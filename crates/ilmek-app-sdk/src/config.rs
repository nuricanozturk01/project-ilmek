use ilmek_protocol::Uuid7;
use ilmek_protocol::wire::Token;
use serde::{Deserialize, Serialize};

/// Start-up parameters the host writes to the backend's stdin (never to argv or the log).
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendConfig {
    pub token: Token,
    pub ipc_port: u16,
    pub community_id: Uuid7,
    pub app_id: Uuid7,
    pub data_dir: String,
}

impl std::fmt::Debug for BackendConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackendConfig")
            .field("app_id", &self.app_id)
            .field("ipc_port", &self.ipc_port)
            .finish_non_exhaustive()
    }
}
