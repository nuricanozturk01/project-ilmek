use ilmek_runtime::{BridgeEvent, EventSink};
use tauri::{AppHandle, Manager};

/// Delivers runtime events to one application window as a JSON *string*, which the page-side
/// SDK parses. Nothing from the application or the network is ever pasted into script text.
pub(crate) struct WindowSink {
    pub(crate) app: AppHandle,
    pub(crate) label: String,
}

impl EventSink for WindowSink {
    fn emit(&self, event: &BridgeEvent) {
        let Ok(json) = serde_json::to_string(event) else {
            return;
        };
        let Ok(literal) = serde_json::to_string(&json) else {
            return;
        };
        if let Some(window) = self.app.get_webview_window(&self.label)
            && let Err(error) = window.eval(format!("window.__ilmekDeliver({literal})"))
        {
            tracing::debug!(%error, "could not deliver an event to {}", self.label);
        }
    }
}
