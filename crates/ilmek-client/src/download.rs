//! Fetching and verifying application packages (README §17.4, PROTOCOL.md §11).
//!
//! Progress counts bytes actually received. Nothing becomes visible until every file's size and
//! SHA-256 and the manifest signature have been verified.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use ilmek_core::session::{ClientError, ClientSession};
use ilmek_core::trust::TrustStore;
use ilmek_identity::PublisherKey;
use ilmek_package::{
    ASSETS_DIR, MANIFEST_FILE, PackageLimits, SIGNATURE_FILE, hash_file, verify_dir,
    verify_manifest,
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
        let key = publisher_key(record)?;
        let package = verify_dir(
            &dir,
            &key,
            Some(record.manifest_hash),
            PackageLimits::default(),
        )?;
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

fn publisher_key(record: &AppRecord) -> Result<PublisherKey, Error> {
    let key = PublisherKey::from_base64url(&record.publisher_public_key.to_string())
        .map_err(|e| Error::PublisherKey(e.to_string()))?;
    if key.key_id() != record.publisher_key_id.to_string() {
        return Err(Error::PublisherKey("key id does not match the key".into()));
    }
    Ok(key)
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
    let key = publisher_key(record)?;
    if connector
        .trust()
        .publisher_key_revoked(&record.publisher_key_id)
    {
        return Err(Error::PublisherRevoked);
    }
    let address = SocketAddr::new(record.endpoint.ip.into(), record.endpoint.port);
    let control = connector
        .connect_host(address, record.server_id, Mode::Control)
        .await?;
    let (manifest, manifest_bytes, signature) = fetch_manifest(&control, record, &key).await?;
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
    fs::write(
        partial.join(SIGNATURE_FILE),
        ilmek_package_signature_text(&signature),
    )?;

    let bulk = connector
        .connect_host(address, record.server_id, Mode::Bulk)
        .await?;
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
        &key,
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

fn ilmek_package_signature_text(signature: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature)
}

async fn fetch_manifest(
    control: &ClientSession,
    record: &AppRecord,
    key: &PublisherKey,
) -> Result<(Manifest, Vec<u8>, Vec<u8>), Error> {
    let request = Message::GetManifest(GetManifest {
        app_id: record.app_id,
        version: record.version,
        manifest_hash: record.manifest_hash,
    });
    let Message::Manifest(payload) = control.request(&request).await? else {
        return Err(Error::UnexpectedReply("expected MANIFEST"));
    };
    if payload.data.app_id != record.app_id
        || payload.data.version != record.version
        || payload.data.publisher_key_id != record.publisher_key_id
    {
        return Err(Error::UnexpectedReply("MANIFEST describes another package"));
    }
    let signature = payload.data.signature.into_bytes();
    let (manifest, _) = verify_manifest(
        &payload.body,
        &signature,
        key,
        Some(record.manifest_hash),
        PackageLimits::default(),
    )?;
    if manifest.app_id != record.app_id || manifest.version != record.version {
        return Err(Error::UnexpectedReply(
            "manifest identity differs from the registry record",
        ));
    }
    Ok((manifest, payload.body, signature))
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
        let existing = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
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

/// Checks that a trust store marks neither the host nor the publisher key as revoked.
pub fn ensure_not_revoked(trust: &TrustStore, record: &AppRecord) -> Result<(), Error> {
    if trust.publisher_key_revoked(&record.publisher_key_id) {
        return Err(Error::PublisherRevoked);
    }
    Ok(())
}
