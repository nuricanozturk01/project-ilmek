use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use ilmek_client::{AppClient, CallResult};
use ilmek_protocol::Uuid7;
use ilmek_protocol::address::Address;
use ilmek_protocol::manifest::{Manifest, Permission};
use ilmek_protocol::wire::AppRecord;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::SDK_SCRIPT;
use crate::assets::{AssetPolicy, AssetResponse};
use crate::bridge::{CallRequest, Failure, Request, Response};
use crate::events::{BridgeEvent, EventSink};
use crate::storage::AppStorage;
use crate::subscription::{self, Subscription};

const DEFAULT_CALL_TIMEOUT: Duration = Duration::from_secs(20);
/// Scheme the shell registers for application content; one host name per application.
const SCHEME: &str = "ilmekapp";

/// One opened application: its verified package, its backend connection, its storage and the
/// bridge its page talks through. A page can reach nothing else.
pub struct AppSession {
    pub(crate) id: Uuid7,
    address: Address,
    record: AppRecord,
    manifest: Manifest,
    client: AppClient,
    pub(crate) storage: AppStorage,
    assets: AssetPolicy,
    pub(crate) sink: Arc<dyn EventSink>,
    connected: AtomicBool,
    subscriptions: Mutex<HashMap<Uuid7, Subscription>>,
    pub(crate) shutdown: CancellationToken,
}

impl std::fmt::Debug for AppSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppSession")
            .field("address", &self.address.to_string())
            .finish_non_exhaustive()
    }
}

pub(crate) struct SessionParts {
    pub(crate) address: Address,
    pub(crate) record: AppRecord,
    pub(crate) manifest: Manifest,
    pub(crate) package_dir: PathBuf,
    pub(crate) client: AppClient,
    pub(crate) storage: AppStorage,
    pub(crate) sink: Arc<dyn EventSink>,
}

impl AppSession {
    pub(crate) fn new(parts: SessionParts) -> Arc<Self> {
        let files = parts
            .manifest
            .files
            .iter()
            .map(|file| (file.path.to_string(), file.size_bytes))
            .collect();
        let assets = AssetPolicy::new(
            parts.package_dir,
            files,
            parts.manifest.entrypoint.to_string(),
        );
        Arc::new(Self {
            id: Uuid7::generate(),
            address: parts.address,
            record: parts.record,
            manifest: parts.manifest,
            client: parts.client,
            storage: parts.storage,
            assets,
            sink: parts.sink,
            connected: AtomicBool::new(true),
            subscriptions: Mutex::new(HashMap::new()),
            shutdown: CancellationToken::new(),
        })
    }

    #[must_use]
    pub fn id(&self) -> Uuid7 {
        self.id
    }

    #[must_use]
    pub fn record(&self) -> &AppRecord {
        &self.record
    }

    #[must_use]
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Host part of this application's private origin.
    #[must_use]
    pub fn host_name(&self) -> String {
        format!("app-{}", self.record.app_id)
    }

    #[must_use]
    pub fn origin(&self) -> String {
        format!("{SCHEME}://{}", self.host_name())
    }

    #[must_use]
    pub fn start_url(&self) -> String {
        format!("{}/{}", self.origin(), self.manifest.entrypoint)
    }

    /// Script run in the page before any of its own code. The identity it carries comes from the
    /// verified package and the registry record, never from the page.
    #[must_use]
    pub fn bootstrap_script(&self) -> String {
        let info = json!({
            "id": self.record.app_id,
            "version": self.record.version,
            "path": self.address.path,
        });
        format!("window.__ILMEK_BOOTSTRAP__ = {info};\n{SDK_SCRIPT}")
    }

    #[must_use]
    pub fn serve(&self, request_path: &str) -> AssetResponse {
        self.assets.serve(request_path)
    }

    #[must_use]
    pub fn allows_navigation(&self, url: &str) -> bool {
        self.assets.allows_navigation(&self.origin(), url)
    }

    fn require(&self, permission: Permission, what: &str) -> Result<(), Failure> {
        if self.manifest.has_permission(permission) {
            Ok(())
        } else {
            Err(Failure::forbidden(&format!(
                "this application's package does not request {what}"
            )))
        }
    }

    /// Handles one page request. Never panics and never leaks internals; failures are data.
    pub async fn handle(self: &Arc<Self>, raw: Value) -> Response {
        let outcome = match Request::parse(raw) {
            Ok(request) => self.dispatch(request).await,
            Err(failure) => Err(failure),
        };
        match outcome {
            Ok(result) => Response::success(result),
            Err(failure) => Response::failure(failure),
        }
    }

    async fn dispatch(self: &Arc<Self>, request: Request) -> Result<Value, Failure> {
        match request {
            Request::Call(call) => {
                self.require(Permission::AppCall, "app.call")?;
                self.call(&call).await
            }
            Request::Subscribe(subscribe) => {
                self.require(Permission::AppSubscribe, "app.subscribe")?;
                let cursor = subscribe
                    .cursor
                    .map(|text| text.parse())
                    .transpose()
                    .map_err(|_| Failure::invalid("cursor is not valid"))?;
                let topic = subscribe
                    .topic
                    .parse()
                    .map_err(|_| Failure::invalid("topic is not valid"))?;
                let id = subscription::start(Arc::clone(self), topic, cursor).await?;
                Ok(json!({ "subscriptionId": id }))
            }
            Request::Unsubscribe(request) => {
                let id: Uuid7 = request
                    .subscription_id
                    .parse()
                    .map_err(|_| Failure::invalid("subscription id"))?;
                self.stop_subscription(id);
                Ok(Value::Null)
            }
            Request::StorageGet(request) => {
                self.require(Permission::StorageLocal, "storage.local")?;
                crate::bridge::valid_key(&request.key)?;
                self.storage.get(request.key).await
            }
            Request::StorageSet(request) => {
                self.require(Permission::StorageLocal, "storage.local")?;
                crate::bridge::valid_key(&request.key)?;
                self.storage
                    .set(request.key, request.value)
                    .await
                    .map(|()| Value::Null)
            }
            Request::StorageRemove(request) => {
                self.require(Permission::StorageLocal, "storage.local")?;
                crate::bridge::valid_key(&request.key)?;
                self.storage.remove(request.key).await.map(|()| Value::Null)
            }
            Request::ConnectionState => Ok(json!(self.connection_label())),
        }
    }

    async fn call(&self, call: &CallRequest) -> Result<Value, Failure> {
        let method = call.validated_method()?;
        let operation_id = call.validated_operation_id()?;
        let body = call.body()?;
        let deadline = call.timeout()?.unwrap_or(DEFAULT_CALL_TIMEOUT);

        let attempt = async {
            if let Some(id) = operation_id {
                self.client.write(method.as_str(), &body, Some(id)).await
            } else {
                self.client.read(method.as_str(), &body).await
            }
        };
        let Ok(outcome) = tokio::time::timeout(deadline, attempt).await else {
            self.note_reachability(false);
            return Err(Failure {
                retryable: true,
                outcome_unknown: call.write,
                operation_id: operation_id.map(|id| id.to_string()),
                ..Failure::plain("TIMEOUT", "no answer in time; the outcome is unknown")
            });
        };
        match outcome {
            Ok(CallResult { body, .. }) => {
                self.note_reachability(true);
                serde_json::from_slice(&body).map_err(|_| {
                    Failure::plain("INTERNAL", "the application answered with invalid JSON")
                })
            }
            Err(error) => {
                let reached = error.remote().is_some();
                self.note_reachability(reached);
                Err(Failure::from_client(&error, call.write, operation_id))
            }
        }
    }

    pub(crate) fn client(&self) -> &AppClient {
        &self.client
    }

    pub(crate) fn connection_label(&self) -> &'static str {
        if self.connected.load(Ordering::Relaxed) {
            "connected"
        } else {
            "disconnected"
        }
    }

    /// Records whether the server could be reached and tells the page when that changes.
    pub(crate) fn note_reachability(&self, reachable: bool) {
        if self.connected.swap(reachable, Ordering::Relaxed) != reachable {
            self.sink.emit(&BridgeEvent::Connection {
                state: self.connection_label(),
            });
        }
    }

    pub(crate) fn register_subscription(&self, id: Uuid7, subscription: Subscription) {
        self.subscriptions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, subscription);
    }

    pub(crate) fn forget_subscription(&self, id: Uuid7) {
        self.subscriptions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id);
    }

    fn stop_subscription(&self, id: Uuid7) {
        if let Some(subscription) = self
            .subscriptions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id)
        {
            subscription.stop.cancel();
        }
    }

    /// Ends every subscription and releases the connection.
    pub fn close(&self) {
        self.shutdown.cancel();
        self.subscriptions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }
}
