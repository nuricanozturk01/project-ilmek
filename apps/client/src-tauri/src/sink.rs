use ilmek_runtime::{BridgeEvent, EventSink};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize)]
struct AppEvent<'a> {
    label: &'a str,
    json: String,
}

/// Hands runtime events of one application tab to the shell UI as a JSON *string*, which the UI
/// forwards to that tab's frame and the page-side SDK parses. Nothing from the application or
/// the network is ever pasted into script text.
pub(crate) struct WindowSink {
    pub(crate) app: AppHandle,
    pub(crate) label: String,
}

impl EventSink for WindowSink {
    fn emit(&self, event: &BridgeEvent) {
        let Ok(json) = serde_json::to_string(event) else {
            return;
        };
        if let Err(error) = self.app.emit(
            "app-event",
            AppEvent {
                label: &self.label,
                json,
            },
        ) {
            tracing::debug!(%error, "could not deliver an event to {}", self.label);
        }
    }
}
