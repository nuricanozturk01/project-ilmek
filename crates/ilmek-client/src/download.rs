//! Fetching and verifying application packages (REQUIREMENTS §17.4, PROTOCOL.md §11).
//!
//! Progress counts bytes actually received. Nothing becomes visible until every file's size and
//! SHA-256 has been verified.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use ilmek_core::session::{ClientError, ClientSession};
use ilmek_package::{
    ASSETS_DIR, MANIFEST_FILE, PackageLimits, hash_file, verify_dir, verify_manifest,
};
use ilmek_protocol::manifest::{Manifest, ManifestFile};
use ilmek_protocol::wire::{
    AppRecord, Cancel, ErrorCode, GetAsset, GetManifest, Hash, Message, Mode,
};
use tokio_util::sync::CancellationToken;

use crate::connector::Connector;
use crate::error::Error;

/// Real byte counts for a running download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: usize,
    pub files_total: usize,
}

/// A fully verified package in the local cache.
#[derive(Debug, Clone)]
pub struct CachedPackage {
    pub dir: PathBuf,
    pub manifest: Manifest,
    pub manifest_hash: Hash,
}

/// On-disk cache of verified packages, bounded by a quota.
#[derive(Debug, Clone)]
pub struct PackageCache {
    root: PathBuf,
    quota_bytes: u64,
}

impl PackageCache {
    #[must_use]
    pub fn new(root: PathBuf, quota_bytes: u64) -> Self {
        Self { root, quota_bytes }
    }

    fn final_dir(&self, record: &AppRecord) -> PathBuf {
        self.root
            .join(record.community_id.to_string())
            .join(record.app_id.to_string())
            .join(format!(
                "{}-{}",
                record.version,
                &record.manifest_hash.to_string()[..12]
            ))
    }

    fn partial_dir(&self, record: &AppRecord) -> PathBuf {
        let name = format!(
            ".partial-{}-{}",
            record.version,
            &record.manifest_hash.to_string()[..12]
        );
        self.final_dir(record).with_file_name(name)
    }

    /// Bytes currently used by cached and partial packages.
    #[must_use]
    pub fn used_bytes(&self) -> u64 {
        directory_size(&self.root)
    }

    /// Returns the verified package if it is already cached.
    pub fn lookup(&self, record: &AppRecord) -> Result<Option<CachedPackage>, Error> {
        let dir = self.final_dir(record);
        if !dir.is_dir() {
            return Ok(None);
        }
        let package = verify_dir(&dir, Some(record.manifest_hash), PackageLimits::default())?;
        Ok(Some(CachedPackage {
            dir,
            manifest: package.manifest,
            manifest_hash: package.manifest_hash,
        }))
    }

    /// Removes a cached package (for "uninstall"/"clear cache").
    pub fn evict(&self, record: &AppRecord) -> Result<(), Error> {
        for dir in [self.final_dir(record), self.partial_dir(record)] {
            match fs::remove_dir_all(&dir) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                    return Err(error.into());
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn directory_size(path: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(meta) if meta.is_dir() => directory_size(&entry.path()),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        })
        .sum()
}

/// Downloads, verifies and caches the package named by `record`.
///
/// Interrupted transfers keep their partial files, and calling again resumes them. Cancelling
/// through `cancel` stops promptly and also keeps the partial data.
pub async fn fetch_package(
    connector: &Connector,
    record: &AppRecord,
    cache: &PackageCache,
    mut on_progress: impl FnMut(Progress) + Send,
    cancel: &CancellationToken,
) -> Result<CachedPackage, Error> {
    if let Some(cached) = cache.lookup(record)? {
        return Ok(cached);
    }
    let address = connector.host_address(&record.endpoint);
    let control = connector.connect_host(address, Mode::Control).await?;
    let (manifest, manifest_bytes) = fetch_manifest(&control, record).await?;
    control.close().await;

    let needed = manifest.total_size_bytes;
    let on_disk = cache.used_bytes();
    if on_disk.saturating_add(needed) > cache.quota_bytes {
        return Err(Error::Quota {
            needed,
            quota: cache.quota_bytes,
        });
    }

    let partial = cache.partial_dir(record);
    fs::create_dir_all(partial.join(ASSETS_DIR))?;
    fs::write(partial.join(MANIFEST_FILE), &manifest_bytes)?;

    let bulk = connector.connect_host(address, Mode::Bulk).await?;
    let mut done = 0u64;
    let total_files = manifest.files.len();
    for (index, file) in manifest.files.iter().enumerate() {
        let report = |bytes_done: u64| Progress {
            bytes_done,
            bytes_total: needed,
            files_done: index,
            files_total: total_files,
        };
        let received = fetch_file(&bulk, record, file, &partial, cancel, |bytes| {
            on_progress(report(done + bytes));
        })
        .await?;
        done += received;
    }
    bulk.close().await;
    on_progress(Progress {
        bytes_done: needed,
        bytes_total: needed,
        files_done: total_files,
        files_total: total_files,
    });

    verify_dir(
        &partial,
        Some(record.manifest_hash),
        PackageLimits::default(),
    )?;
    let destination = cache.final_dir(record);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&partial, &destination)?;
    Ok(CachedPackage {
        dir: destination,
        manifest,
        manifest_hash: record.manifest_hash,
    })
}

async fn fetch_manifest(
    control: &ClientSession,
    record: &AppRecord,
) -> Result<(Manifest, Vec<u8>), Error> {
    let request = Message::GetManifest(GetManifest {
        app_id: record.app_id,
        version: record.version,
        manifest_hash: record.manifest_hash,
    });
    let Message::Manifest(payload) = control.request(&request).await? else {
        return Err(Error::UnexpectedReply("expected MANIFEST"));
    };
    if payload.data.app_id != record.app_id || payload.data.version != record.version {
        return Err(Error::UnexpectedReply("MANIFEST describes another package"));
    }
    let (manifest, _) = verify_manifest(
        &payload.body,
        Some(record.manifest_hash),
        PackageLimits::default(),
    )?;
    if manifest.app_id != record.app_id || manifest.version != record.version {
        return Err(Error::UnexpectedReply(
            "manifest identity differs from the registry record",
        ));
    }
    Ok((manifest, payload.body))
}

/// Fetches one file into `partial/assets/`, resuming a `.part` file if one exists.
/// Returns the file's full size.
async fn fetch_file(
    bulk: &ClientSession,
    record: &AppRecord,
    file: &ManifestFile,
    partial: &Path,
    cancel: &CancellationToken,
    mut on_bytes: impl FnMut(u64) + Send,
) -> Result<u64, Error> {
    let target = partial.join(ASSETS_DIR).join(file.path.as_str());
    if target.exists() {
        let digest = hash_file(&target)?;
        if digest.size == file.size_bytes && digest.sha256 == file.sha256 {
            on_bytes(file.size_bytes);
            return Ok(file.size_bytes);
        }
        fs::remove_file(&target)?;
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    let part = target.with_extension(format!(
        "{}.part",
        target.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));

    for attempt in 0..2 {
        let existing = fs::metadata(&part).map_or(0, |m| m.len());
        let offset = if existing <= file.size_bytes {
            existing
        } else {
            0
        };
        if offset == 0 && existing != 0 {
            fs::remove_file(&part)?;
        }
        stream_into(bulk, record, file, &part, offset, cancel, &mut on_bytes).await?;

        let digest = hash_file(&part)?;
        if digest.size == file.size_bytes && digest.sha256 == file.sha256 {
            fs::rename(&part, &target)?;
            return Ok(file.size_bytes);
        }
        // A bad resume must not be trusted again: start this file over once, then give up.
        fs::remove_file(&part)?;
        if attempt == 1 {
            return Err(Error::Integrity(file.path.to_string()));
        }
    }
    Err(Error::Integrity(file.path.to_string()))
}

async fn stream_into(
    bulk: &ClientSession,
    record: &AppRecord,
    file: &ManifestFile,
    part: &Path,
    offset: u64,
    cancel: &CancellationToken,
    on_bytes: &mut (impl FnMut(u64) + Send),
) -> Result<(), Error> {
    let mut output: File = OpenOptions::new().create(true).append(true).open(part)?;
    let request = Message::GetAsset(GetAsset {
        app_id: record.app_id,
        version: record.version,
        manifest_hash: record.manifest_hash,
        path: file.path.clone(),
        asset_hash: file.sha256,
        offset,
    });
    let mut stream = bulk.open_stream(&request).await?;
    let mut next_offset = offset;
    let mut next_sequence = 0u64;
    let mut started = false;
    on_bytes(offset);

    loop {
        let frame = tokio::select! {
            () = cancel.cancelled() => {
                let cancel_request = Message::Cancel(Cancel { target_request_id: stream.request_id() });
                let _ = bulk.request_within(&cancel_request, std::time::Duration::from_secs(2)).await;
                return Err(Error::Cancelled);
            }
            frame = stream.next_within(std::time::Duration::from_secs(30)) => frame,
        };
        match frame {
            Ok(Some(Message::AssetInfo(info))) => {
                if started
                    || info.offset != offset
                    || info.total_size != file.size_bytes
                    || info.asset_hash != file.sha256
                {
                    return Err(Error::UnexpectedReply(
                        "ASSET_INFO does not match the request",
                    ));
                }
                started = true;
            }
            Ok(Some(Message::AssetChunk(chunk))) => {
                if !started
                    || chunk.data.sequence != next_sequence
                    || chunk.data.offset != next_offset
                {
                    return Err(Error::UnexpectedReply("asset chunks out of sequence"));
                }
                let end = next_offset + chunk.body.len() as u64;
                if end > file.size_bytes {
                    return Err(Error::UnexpectedReply(
                        "asset chunk beyond the declared size",
                    ));
                }
                output.write_all(&chunk.body)?;
                next_offset = end;
                next_sequence += 1;
                on_bytes(next_offset);
            }
            Ok(Some(Message::AssetEnd(end))) => {
                if !started
                    || end.total_sent != file.size_bytes - offset
                    || end.asset_hash != file.sha256
                {
                    return Err(Error::UnexpectedReply(
                        "ASSET_END does not match the transfer",
                    ));
                }
                output.sync_all()?;
                return Ok(());
            }
            Ok(Some(_)) => {
                return Err(Error::UnexpectedReply(
                    "unexpected frame in an asset transfer",
                ));
            }
            Ok(None) => {
                return Err(Error::UnexpectedReply(
                    "asset transfer ended without ASSET_END",
                ));
            }
            Err(ClientError::Remote(error)) if error.code == ErrorCode::Cancelled => {
                return Err(Error::Cancelled);
            }
            Err(error) => return Err(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    //! A hostile host that follows the handshake but misbehaves while streaming an asset
    //! (PROTOCOL.md §11, P13). Runs over an in-memory connection with the real session code on
    //! both ends.

    use std::sync::Arc;
    use std::time::Duration;

    use ilmek_core::clock::SystemClock;
    use ilmek_core::session::{
        ClientParams, ClientSession, ConnectionParams, Handler, Reply, ServerSessionConfig,
        serve_connection,
    };
    use ilmek_protocol::Uuid7;
    use ilmek_protocol::digest::sha256;
    use ilmek_protocol::wire::{
        AssetChunk, AssetEnd, AssetInfo, Message, Mode, Payload, ServerRole, Version,
    };
    use tokio::io::duplex;

    use super::*;

    const FILE_LEN: usize = 3000;
    const CHUNK_LEN: usize = 1000;

    #[derive(Clone, Copy, Debug)]
    enum Behavior {
        Honest,
        /// Chunk sequence numbers jump from 0 to 2.
        SkippedSequence,
        /// The second chunk claims an offset that does not follow the first.
        WrongChunkOffset,
        /// More bytes than the manifest declares.
        BeyondDeclaredSize,
        /// `ASSET_INFO` ignores the requested resume offset.
        InfoIgnoresOffset,
        /// `ASSET_END` reports a different byte count.
        WrongTotalSent,
        /// A chunk arrives before `ASSET_INFO`.
        ChunkBeforeInfo,
        /// Everything is consistent, but the bytes are not the declared file.
        WrongBytes,
    }

    struct Hostile {
        content: Vec<u8>,
        behavior: Behavior,
    }

    impl Handler for Hostile {
        async fn handle(&self, request: Message, reply: Reply) {
            let Message::GetAsset(get) = request else {
                return;
            };
            let offset = usize::try_from(get.offset).unwrap();
            let info = AssetInfo {
                app_id: get.app_id,
                version: get.version,
                path: get.path.clone(),
                asset_hash: get.asset_hash,
                total_size: self.content.len() as u64,
                offset: if matches!(self.behavior, Behavior::InfoIgnoresOffset) {
                    0
                } else {
                    get.offset
                },
                chunk_size: CHUNK_LEN as u64,
            };
            let chunk = |sequence: u64, at: u64, body: Vec<u8>| {
                Message::AssetChunk(Payload {
                    data: AssetChunk {
                        sequence,
                        offset: at,
                    },
                    body,
                })
            };
            let mut body = self.content[offset..].to_vec();
            if matches!(self.behavior, Behavior::WrongBytes) {
                body[0] ^= 0xff;
            }

            if matches!(self.behavior, Behavior::ChunkBeforeInfo) {
                let _ = reply.send(&chunk(0, get.offset, body)).await;
                return;
            }
            let _ = reply.send(&Message::AssetInfo(info)).await;
            let mut sent = 0u64;
            for (index, part) in body.chunks(CHUNK_LEN).enumerate() {
                let mut sequence = index as u64;
                let mut at = get.offset + sent;
                match self.behavior {
                    Behavior::SkippedSequence if index == 1 => sequence += 1,
                    Behavior::WrongChunkOffset if index == 1 => at += 1,
                    _ => {}
                }
                let _ = reply.send(&chunk(sequence, at, part.to_vec())).await;
                sent += part.len() as u64;
            }
            if matches!(self.behavior, Behavior::BeyondDeclaredSize) {
                let _ = reply
                    .send(&chunk(
                        u64::try_from(body.len().div_ceil(CHUNK_LEN)).unwrap(),
                        get.offset + sent,
                        vec![0; CHUNK_LEN],
                    ))
                    .await;
            }
            let total_sent = if matches!(self.behavior, Behavior::WrongTotalSent) {
                sent + 1
            } else {
                sent
            };
            let _ = reply
                .finish(&Message::AssetEnd(AssetEnd {
                    asset_hash: get.asset_hash,
                    total_sent,
                }))
                .await;
        }
    }

    struct Rig {
        bulk: ClientSession,
        record: AppRecord,
        file: ManifestFile,
        content: Vec<u8>,
        dir: tempfile::TempDir,
    }

    fn content() -> Vec<u8> {
        (0..FILE_LEN)
            .map(|n| u8::try_from(n % 251).unwrap())
            .collect()
    }

    async fn rig(behavior: Behavior) -> Rig {
        let content = content();
        let community = Uuid7::generate();
        let server_id = Uuid7::generate();
        let config = ServerSessionConfig::new(
            server_id,
            ServerRole::Host,
            community,
            Arc::new(SystemClock),
        );
        let (near, far) = duplex(256 * 1024);
        tokio::spawn(serve_connection(
            far,
            ConnectionParams {
                remote_ip: None,
                config: Arc::new(config),
                handler: Arc::new(Hostile {
                    content: content.clone(),
                    behavior,
                }),
                shutdown: CancellationToken::new(),
            },
        ));
        let bulk = ClientSession::open(
            near,
            ClientParams {
                mode: Mode::Bulk,
                community_id: community,
                client_id: Uuid7::generate(),
                expected_role: ServerRole::Host,
                client_name: "test".into(),
                client_version: Version::new(1, 0, 0),
                request_timeout: Duration::from_secs(5),
            },
        )
        .await
        .unwrap();

        let app_id = Uuid7::generate();
        let record: AppRecord = serde_json::from_value(serde_json::json!({
            "community_id": community.to_string(),
            "namespace": "nuri",
            "slug": "forum",
            "app_id": app_id.to_string(),
            "publisher_id": Uuid7::generate().to_string(),
            "server_id": server_id.to_string(),
            "endpoint": { "ip": "127.0.0.1", "port": 47401 },
            "version": "1.0.0",
            "manifest_hash": Hash::from_bytes([7; 32]).to_string(),
            "name": "Forum",
            "description": "test",
            "revision": 1,
            "lease_expires_at_ms": 0,
            "availability": "recent_heartbeat",
        }))
        .unwrap();
        let file = ManifestFile {
            path: "assets/blob.bin".parse().unwrap(),
            size_bytes: FILE_LEN as u64,
            sha256: Hash::from_bytes(sha256(&content)),
        };
        Rig {
            bulk,
            record,
            file,
            content,
            dir: tempfile::tempdir().unwrap(),
        }
    }

    async fn run_stream(rig: &Rig, offset: u64) -> (Result<(), Error>, Vec<u8>) {
        let part = rig.dir.path().join("blob.part");
        if offset > 0 {
            let start = usize::try_from(offset).unwrap();
            fs::write(&part, &rig.content[..start]).unwrap();
        }
        let result = stream_into(
            &rig.bulk,
            &rig.record,
            &rig.file,
            &part,
            offset,
            &CancellationToken::new(),
            &mut |_| {},
        )
        .await;
        (result, fs::read(&part).unwrap_or_default())
    }

    #[tokio::test]
    async fn an_honest_host_completes_fresh_and_resumed_transfers() {
        let fresh = rig(Behavior::Honest).await;
        let (result, bytes) = run_stream(&fresh, 0).await;
        result.unwrap();
        assert_eq!(bytes, fresh.content);

        let resumed = rig(Behavior::Honest).await;
        let (result, bytes) = run_stream(&resumed, 1500).await;
        result.unwrap();
        assert_eq!(bytes, resumed.content);
    }

    #[tokio::test]
    async fn chunk_sequence_offset_size_and_total_deviations_are_rejected() {
        for behavior in [
            Behavior::SkippedSequence,
            Behavior::WrongChunkOffset,
            Behavior::BeyondDeclaredSize,
            Behavior::WrongTotalSent,
            Behavior::ChunkBeforeInfo,
        ] {
            let rig = rig(behavior).await;
            let (result, _) = run_stream(&rig, 0).await;
            assert!(
                matches!(result, Err(Error::UnexpectedReply(_))),
                "{behavior:?} was not rejected: {result:?}"
            );
        }
    }

    #[tokio::test]
    async fn a_host_that_ignores_the_resume_offset_is_rejected() {
        let rig = rig(Behavior::InfoIgnoresOffset).await;
        let (result, _) = run_stream(&rig, 1000).await;
        assert!(
            matches!(result, Err(Error::UnexpectedReply(_))),
            "{result:?}"
        );
    }

    #[tokio::test]
    async fn bytes_that_do_not_match_the_declared_hash_are_never_accepted() {
        let rig = rig(Behavior::WrongBytes).await;
        let partial = rig.dir.path().join("pkg");
        fs::create_dir_all(partial.join(ASSETS_DIR).join("assets")).unwrap();
        let result = fetch_file(
            &rig.bulk,
            &rig.record,
            &rig.file,
            &partial,
            &CancellationToken::new(),
            |_| {},
        )
        .await;
        assert!(matches!(result, Err(Error::Integrity(_))), "{result:?}");
        let target = partial.join(ASSETS_DIR).join("assets/blob.bin");
        assert!(!target.exists(), "a file with the wrong hash was activated");
        let leftovers: Vec<_> = fs::read_dir(partial.join(ASSETS_DIR).join("assets"))
            .unwrap()
            .flatten()
            .collect();
        assert!(leftovers.is_empty(), "a bad partial file was kept");
    }
}
