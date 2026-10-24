//! Publishing an installed application in the registry.

use ilmek_client::{Connector, Error as ClientError, RegistryClient};
use ilmek_package::MANIFEST_FILE;
use ilmek_protocol::Uuid7;
use ilmek_protocol::manifest::{DEFAULT_MAX_TOTAL_BYTES, Manifest};
use ilmek_protocol::wire::{AppRecord, ErrorCode, RegisterApp, UnregisterApp, UpdateApp};
use thiserror::Error;

use crate::error::HostError;
use crate::store::Store;

#[derive(Debug, Error)]
pub enum PublishError {
    #[error(transparent)]
    Host(#[from] HostError),
    #[error(transparent)]
    Registry(#[from] ClientError),
}

/// Registers the active version of `app_id`, or updates the existing record.
///
/// `connector` is the *publisher's* device; `host_server_id` is the peer id of the host that
/// serves the application.
pub async fn publish(
    connector: &Connector,
    store: &Store,
    app_id: Uuid7,
    host_server_id: Uuid7,
) -> Result<AppRecord, PublishError> {
    let app = store.app(app_id).await?.ok_or(HostError::UnknownApp)?;
    let version = store
        .version(app_id, app.active_version)
        .await?
        .ok_or(HostError::UnknownVersion)?;
    let path = version.directory.join(MANIFEST_FILE);
    let bytes = std::fs::read(&path).map_err(|e| HostError::io(&path, e))?;
    let manifest = Manifest::parse(&bytes, DEFAULT_MAX_TOTAL_BYTES)
        .map_err(|_| HostError::Corrupt("stored manifest"))?;

    let registry = RegistryClient::connect(connector).await?;
    let existing = match registry.resolve(&manifest.namespace, &manifest.slug).await {
        Ok(record) if record.app_id == app_id => Some(record),
        Ok(_) => {
            return Err(HostError::Conflict("that address belongs to another application").into());
        }
        Err(error) if error.remote_code() == Some(ErrorCode::NotFound) => None,
        Err(error) => return Err(error.into()),
    };
    let record = match existing {
        Some(current) => {
            registry
                .update_app(UpdateApp {
                    operation_id: Uuid7::generate(),
                    app_id,
                    expected_revision: current.revision,
                    server_id: host_server_id,
                    version: manifest.version,
                    manifest_hash: version.manifest_hash,
                    name: manifest.name.clone(),
                    description: manifest.description.clone(),
                })
                .await?
        }
        None => {
            registry
                .register_app(RegisterApp {
                    operation_id: Uuid7::generate(),
                    namespace: manifest.namespace.clone(),
                    slug: manifest.slug.clone(),
                    app_id,
                    server_id: host_server_id,
                    version: manifest.version,
                    manifest_hash: version.manifest_hash,
                    name: manifest.name.clone(),
                    description: manifest.description.clone(),
                })
                .await?
        }
    };
    registry.close().await;
    Ok(record)
}

/// Removes the registry record; the host keeps serving until the app is stopped.
pub async fn unpublish(
    connector: &Connector,
    store: &Store,
    app_id: Uuid7,
) -> Result<(), PublishError> {
    let app = store.app(app_id).await?.ok_or(HostError::UnknownApp)?;
    let registry = RegistryClient::connect(connector).await?;
    let record = registry.resolve(&app.namespace, &app.slug).await?;
    registry
        .unregister_app(UnregisterApp {
            operation_id: Uuid7::generate(),
            app_id,
            expected_revision: record.revision,
        })
        .await?;
    registry.close().await;
    Ok(())
}
