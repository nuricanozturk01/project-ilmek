//! The client runtime. It owns everything an application page may touch: the bridge, the
//! per-application storage, and the content it is allowed to load. The windowing shell around
//! it (Tauri on desktop) stays thin and holds no policy.

mod assets;
mod bridge;
mod client;
mod error;
mod events;
mod session;
mod storage;
mod subscription;

pub use assets::{AssetResponse, CONTENT_SECURITY_POLICY};
pub use bridge::{
    CallRequest, Failure, KeyRequest, Request, Response, SetRequest, SubscribeRequest,
    UnsubscribeRequest,
};
pub use client::{CachedApp, CatalogEntry, CatalogPage, Client, Status};
pub use error::RuntimeError;
pub use events::{BridgeEvent, EventSink};
pub use session::AppSession;

/// The page-side SDK (built from packages/sdk-js), injected into every application page.
pub const SDK_SCRIPT: &str = include_str!("../assets/ilmek-sdk.iife.js");
