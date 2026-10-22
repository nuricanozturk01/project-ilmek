//! The ILMEK/1 host endpoint: manifests, assets, calls and event subscriptions.

use std::collections::HashMap;
use std::io::SeekFrom;
use std::sync::{Arc, Mutex, PoisonError};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ilmek_core::session::{Handler, Reply};
use ilmek_package::{ASSETS_DIR, MANIFEST_FILE, SIGNATURE_FILE};
use ilmek_protocol::Uuid7;
use ilmek_protocol::digest::payload_hash;
use ilmek_protocol::ipc::{IpcCall, IpcMessage, IpcSubscribe};
use ilmek_protocol::manifest::{DEFAULT_MAX_TOTAL_BYTES, Manifest, Permission};
use ilmek_protocol::wire::{
    AppCall, AppResult, AssetChunk, AssetEnd, AssetInfo, ErrorCode, EventAck, EventAcked, GetAsset,
    GetManifest, ManifestInfo, Message, Payload, Subscribe, SubscriptionRef, Token, Version,
};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::link::{BackendLink, LinkError};
use crate::slots::Slots;
use crate::store::{AppRow, Desired, Store, VersionRow};
use crate::subscriptions::{Entry, Subscriptions};
use crate::supervisor::Env;

const ASSET_CHUNK: usize = 65_536;

type ManifestCache = Mutex<HashMap<(Uuid7, Version), Arc<Manifest>>>;

struct Inner {
    env: Arc<Env>,
    slots: Arc<Slots>,
    subscriptions: Arc<Subscriptions>,
    manifests: ManifestCache,
}

/// Cloneable handle that serves every host connection.
#[derive(Clone)]
pub struct HostService {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for HostService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostService").finish_non_exhaustive()
    }
}

struct Failure {
    code: ErrorCode,
    text: &'static str,
    retryable: bool,
}

impl Failure {
    const fn new(code: ErrorCode, text: &'static str) -> Self {
        Self {
            code,
            text,
            retryable: false,
        }
    }

    const fn retryable(code: ErrorCode, text: &'static str) -> Self {
        Self {
            code,
            text,
            retryable: true,
        }
    }
}

type Outcome = Result<(), Failure>;

impl HostService {
    pub(crate) fn new(env: Arc<Env>, slots: Arc<Slots>) -> Self {
        Self {
            inner: Arc::new(Inner::new(env, slots)),
        }
    }
}

impl Inner {
    fn new(env: Arc<Env>, slots: Arc<Slots>) -> Self {
        Self {
            env,
            slots,
            subscriptions: Arc::new(Subscriptions::default()),
            manifests: Mutex::new(HashMap::new()),
        }
    }

    fn store(&self) -> &Store {
        &self.env.store
    }

    /// The application, if this host is serving it right now.
    async fn served(&self, app_id: Uuid7) -> Result<AppRow, Failure> {
        let app = self
            .store()
            .app(app_id)
            .await
            .map_err(|_| Failure::retryable(ErrorCode::Internal, "storage error"))?
            .ok_or(Failure::new(ErrorCode::NotFound, "no such application"))?;
        if app.desired != Desired::Running {
            return Err(Failure::new(
                ErrorCode::AppUnavailable,
                "application is stopped",
            ));
        }
        Ok(app)
    }

    async fn version_of(&self, app_id: Uuid7, version: Version) -> Result<VersionRow, Failure> {
        self.store()
            .version(app_id, version)
            .await
            .map_err(|_| Failure::retryable(ErrorCode::Internal, "storage error"))?
            .ok_or(Failure::new(ErrorCode::NotFound, "no such version"))
    }

    fn manifest_of(&self, app_id: Uuid7, row: &VersionRow) -> Result<Arc<Manifest>, Failure> {
        if let Some(found) = self
            .manifests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&(app_id, row.version))
        {
            return Ok(Arc::clone(found));
        }
        let bytes = std::fs::read(row.directory.join(MANIFEST_FILE))
            .map_err(|_| Failure::new(ErrorCode::Internal, "package files are unreadable"))?;
        let manifest = Manifest::parse(&bytes, DEFAULT_MAX_TOTAL_BYTES)
            .map_err(|_| Failure::new(ErrorCode::Internal, "stored manifest is invalid"))?;
        let manifest = Arc::new(manifest);
        self.manifests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert((app_id, row.version), Arc::clone(&manifest));
        Ok(manifest)
    }

    async fn get_manifest(&self, request: GetManifest, reply: &Reply) -> Outcome {
        self.served(request.app_id).await?;
        let row = self.version_of(request.app_id, request.version).await?;
        if row.manifest_hash != request.manifest_hash {
            return Err(Failure::new(
                ErrorCode::VersionConflict,
                "manifest hash differs",
            ));
        }
        let manifest = self.manifest_of(request.app_id, &row)?;
        let bytes = std::fs::read(row.directory.join(MANIFEST_FILE))
            .map_err(|_| Failure::new(ErrorCode::Internal, "package files are unreadable"))?;
        let signature = std::fs::read_to_string(row.directory.join(SIGNATURE_FILE))
            .ok()
            .and_then(|text| URL_SAFE_NO_PAD.decode(text.trim()).ok())
            .ok_or(Failure::new(
                ErrorCode::Internal,
                "package signature is unreadable",
            ))?;
        let message = Message::Manifest(Payload {
            data: ManifestInfo {
                app_id: request.app_id,
                version: request.version,
                manifest_hash: row.manifest_hash,
                publisher_key_id: manifest.publisher_key_id,
                signature: Token::new(signature),
            },
            body: bytes,
        });
        reply
            .finish(&message)
            .await
            .map_err(|_| Failure::new(ErrorCode::Internal, "connection closed"))
    }

    async fn get_asset(&self, request: GetAsset, reply: &Reply) -> Outcome {
        self.served(request.app_id).await?;
        let row = self.version_of(request.app_id, request.version).await?;
        if row.manifest_hash != request.manifest_hash {
            return Err(Failure::new(
                ErrorCode::VersionConflict,
                "manifest hash differs",
            ));
        }
        let manifest = self.manifest_of(request.app_id, &row)?;
        let file = manifest
            .files
            .iter()
            .find(|f| f.path == request.path)
            .ok_or(Failure::new(
                ErrorCode::NotFound,
                "no such file in the package",
            ))?;
        if file.sha256 != request.asset_hash {
            return Err(Failure::new(
                ErrorCode::HashMismatch,
                "asset hash differs from the manifest",
            ));
        }
        if request.offset > file.size_bytes {
            return Err(Failure::new(
                ErrorCode::InvalidMessage,
                "offset beyond the end of the file",
            ));
        }

        let disk = row.directory.join(ASSETS_DIR).join(request.path.as_str());
        let mut handle = tokio::fs::File::open(&disk)
            .await
            .map_err(|_| Failure::new(ErrorCode::Internal, "package files are unreadable"))?;
        handle
            .seek(SeekFrom::Start(request.offset))
            .await
            .map_err(|_| Failure::new(ErrorCode::Internal, "package files are unreadable"))?;

        let closed = || Failure::new(ErrorCode::Internal, "connection closed");
        let info = AssetInfo {
            app_id: request.app_id,
            version: request.version,
            path: request.path.clone(),
            asset_hash: file.sha256,
            total_size: file.size_bytes,
            offset: request.offset,
            chunk_size: ASSET_CHUNK as u64,
        };
        reply
            .send(&Message::AssetInfo(info))
            .await
            .map_err(|_| closed())?;

        let mut position = request.offset;
        let mut sequence = 0u64;
        let mut buffer = vec![0u8; ASSET_CHUNK];
        while position < file.size_bytes {
            if reply.cancelled().is_cancelled() {
                return Ok(());
            }
            let wanted = usize::try_from(file.size_bytes - position)
                .unwrap_or(ASSET_CHUNK)
                .min(ASSET_CHUNK);
            let read = handle
                .read(&mut buffer[..wanted])
                .await
                .map_err(|_| Failure::new(ErrorCode::Internal, "package files are unreadable"))?;
            if read == 0 {
                return Err(Failure::new(
                    ErrorCode::Internal,
                    "package file is shorter than its manifest",
                ));
            }
            let chunk = Message::AssetChunk(Payload {
                data: AssetChunk {
                    sequence,
                    offset: position,
                },
                body: buffer[..read].to_vec(),
            });
            if reply.send(&chunk).await.is_err() {
                return Ok(());
            }
            position += read as u64;
            sequence += 1;
        }
        let end = AssetEnd {
            asset_hash: file.sha256,
            total_sent: file.size_bytes - request.offset,
        };
        let _ = reply.finish(&Message::AssetEnd(end)).await;
        Ok(())
    }

    /// Checks everything common to calls and subscriptions and returns the backend link.
    async fn backend_for(
        &self,
        app_id: Uuid7,
        client_version: Version,
        permission: Permission,
    ) -> Result<Arc<BackendLink>, Failure> {
        let app = self.served(app_id).await?;
        if !app.backend_required {
            return Err(Failure::new(
                ErrorCode::NotFound,
                "application has no backend",
            ));
        }
        let accepted = client_version == app.active_version
            || self
                .store()
                .backend(app_id)
                .await
                .map_err(|_| Failure::retryable(ErrorCode::Internal, "storage error"))?
                .is_some_and(|b| b.accepted_versions.contains(&client_version));
        if !accepted {
            return Err(Failure::new(
                ErrorCode::AppVersionUnsupported,
                "reload the application",
            ));
        }
        let row = self.version_of(app_id, client_version).await?;
        if !self.manifest_of(app_id, &row)?.has_permission(permission) {
            return Err(Failure::new(
                ErrorCode::Forbidden,
                "package lacks the required permission",
            ));
        }
        self.slots
            .get(app_id)
            .and_then(|slot| slot.link())
            .ok_or(Failure::retryable(
                ErrorCode::AppUnavailable,
                "backend is not running",
            ))
    }

    async fn app_call(&self, request: Payload<AppCall>, reply: &Reply) -> Outcome {
        let Payload { data, body } = request;
        let principal = reply
            .session()
            .peer
            .as_ref()
            .map(|p| p.id)
            .ok_or(Failure::new(
                ErrorCode::Unauthorized,
                "no authenticated peer",
            ))?;
        let link = self
            .backend_for(data.app_id, data.client_app_version, Permission::AppCall)
            .await?;
        let expected = payload_hash(data.app_id, data.method.as_str(), &body);
        if expected != *data.payload_hash.as_bytes() {
            return Err(Failure::new(
                ErrorCode::HashMismatch,
                "payload hash does not match the body",
            ));
        }
        let slot = self.slots.get(data.app_id).ok_or(Failure::new(
            ErrorCode::AppUnavailable,
            "backend is not running",
        ))?;
        let _permit = slot.gate.enter().await.ok_or(Failure::retryable(
            ErrorCode::ResourceExhausted,
            "application is busy",
        ))?;

        let call = IpcCall {
            app_id: data.app_id,
            client_app_version: data.client_app_version,
            method: data.method,
            operation_id: data.operation_id,
            payload_hash: data.payload_hash,
            principal_id: principal,
        };
        let answer =
            tokio::time::timeout(self.env.settings.call_deadline(), link.call(call, body)).await;
        let message = match answer {
            Err(_) => {
                return Err(Failure::retryable(
                    ErrorCode::Timeout,
                    "no answer yet; repeat with the same operation id",
                ));
            }
            Ok(Err(LinkError::Closed)) => {
                return Err(Failure::retryable(
                    ErrorCode::AppUnavailable,
                    "backend went away; repeat with the same operation id",
                ));
            }
            Ok(Ok(IpcMessage::Result(result))) => {
                if result.data.app_id != data.app_id
                    || result.data.operation_id != data.operation_id
                {
                    return Err(Failure::new(
                        ErrorCode::Internal,
                        "backend answered for another call",
                    ));
                }
                Message::AppResult(Payload {
                    data: AppResult { ..result.data },
                    body: result.body,
                })
            }
            Ok(Ok(IpcMessage::Error(error))) => Message::Error(error),
            Ok(Err(_) | Ok(_)) => {
                return Err(Failure::new(ErrorCode::Internal, "backend protocol error"));
            }
        };
        let _ = reply.finish(&message).await;
        Ok(())
    }

    async fn subscribe(self: &Arc<Self>, request: Subscribe, reply: Reply) -> Outcome {
        let session = reply.session();
        let principal = session.peer.as_ref().map(|p| p.id).ok_or(Failure::new(
            ErrorCode::Unauthorized,
            "no authenticated peer",
        ))?;
        let link = self
            .backend_for(
                request.app_id,
                request.client_app_version,
                Permission::AppSubscribe,
            )
            .await?;
        if u64::try_from(self.subscriptions.count(session.session_id)).unwrap_or(u64::MAX)
            >= session.max_subscriptions
        {
            return Err(Failure::retryable(
                ErrorCode::ResourceExhausted,
                "too many subscriptions",
            ));
        }
        let started = link
            .subscribe(IpcSubscribe {
                app_id: request.app_id,
                client_app_version: request.client_app_version,
                topic: request.topic,
                cursor: request.cursor,
                principal_id: principal,
            })
            .await
            .map_err(|_| Failure::retryable(ErrorCode::AppUnavailable, "backend is not running"))?;

        let subscribed = match started.first {
            IpcMessage::Subscribed(subscribed) => subscribed,
            IpcMessage::Error(error) => {
                let _ = reply.finish(&Message::Error(error)).await;
                return Ok(());
            }
            _ => return Err(Failure::new(ErrorCode::Internal, "backend protocol error")),
        };
        let subscription_id = subscribed.subscription_id;
        let entry = Entry {
            session_id: session.session_id,
            link: Arc::clone(&link),
            stop: tokio_util::sync::CancellationToken::new(),
        };
        let stop = entry.stop.clone();
        if !self
            .subscriptions
            .add(subscription_id, entry, session.max_subscriptions)
        {
            let _ = link.unsubscribe(subscription_id).await;
            return Err(Failure::retryable(
                ErrorCode::ResourceExhausted,
                "too many subscriptions",
            ));
        }
        if reply.send(&Message::Subscribed(subscribed)).await.is_err() {
            self.subscriptions.remove(subscription_id);
            return Ok(());
        }
        let service = Arc::clone(self);
        tokio::spawn(forward_events(
            service,
            reply,
            link,
            started.events,
            subscription_id,
            stop,
        ));
        Ok(())
    }

    async fn event_ack(&self, ack: EventAck, reply: &Reply) -> Outcome {
        let entry = self
            .subscriptions
            .owned(ack.subscription_id, reply.session().session_id)
            .ok_or(Failure::new(ErrorCode::NotFound, "no such subscription"))?;
        let sequence = ack.sequence;
        let subscription_id = ack.subscription_id;
        let message = match entry.link.ack(ack).await {
            Ok(IpcMessage::EventAcked(_)) => Message::EventAcked(EventAcked {
                subscription_id,
                sequence,
            }),
            Ok(IpcMessage::Error(error)) => Message::Error(error),
            _ => {
                return Err(Failure::retryable(
                    ErrorCode::AppUnavailable,
                    "backend is not running",
                ));
            }
        };
        let _ = reply.finish(&message).await;
        Ok(())
    }

    async fn unsubscribe(&self, request: SubscriptionRef, reply: &Reply) -> Outcome {
        if let Some(entry) = self
            .subscriptions
            .owned(request.subscription_id, reply.session().session_id)
        {
            entry.stop.cancel();
            let _ = entry.link.unsubscribe(request.subscription_id).await;
            self.subscriptions.remove(request.subscription_id);
        }
        let _ = reply.finish(&Message::Unsubscribed(request)).await;
        Ok(())
    }
}

/// Relays a backend's events to the subscribed client until either side ends.
async fn forward_events(
    service: Arc<Inner>,
    reply: Reply,
    link: Arc<BackendLink>,
    mut events: tokio::sync::mpsc::Receiver<IpcMessage>,
    subscription_id: Uuid7,
    stop: tokio_util::sync::CancellationToken,
) {
    let session_closed = reply.session().closed.clone();
    loop {
        tokio::select! {
            () = stop.cancelled() => break,
            () = session_closed.cancelled() => {
                let _ = link.unsubscribe(subscription_id).await;
                break;
            }
            next = events.recv() => match next {
                Some(IpcMessage::Event(event)) => {
                    if reply.send(&Message::Event(event)).await.is_err() {
                        break;
                    }
                }
                Some(IpcMessage::Error(error)) => {
                    let _ = reply.finish(&Message::Error(error)).await;
                    break;
                }
                Some(_) => {}
                None => {
                    let _ = reply.fail(ErrorCode::AppUnavailable, "backend is not running", true).await;
                    break;
                }
            }
        }
    }
    service.subscriptions.remove(subscription_id);
}

impl Handler for HostService {
    async fn handle(&self, request: Message, reply: Reply) {
        let inner = &self.inner;
        let outcome = match request {
            Message::GetManifest(r) => inner.get_manifest(r, &reply).await,
            Message::GetAsset(r) => inner.get_asset(r, &reply).await,
            Message::AppCall(r) => inner.app_call(r, &reply).await,
            Message::Subscribe(r) => inner.subscribe(r, reply.clone()).await,
            Message::EventAck(r) => inner.event_ack(r, &reply).await,
            Message::Unsubscribe(r) => inner.unsubscribe(r, &reply).await,
            _ => Err(Failure::new(
                ErrorCode::InvalidMessage,
                "message not served by a host",
            )),
        };
        if let Err(failure) = outcome {
            let _ = reply
                .fail(failure.code, failure.text, failure.retryable)
                .await;
        }
    }
}
