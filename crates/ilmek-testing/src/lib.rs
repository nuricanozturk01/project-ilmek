//! Test fixtures. Production code never depends on this crate.
#![expect(
    clippy::unwrap_used,
    clippy::missing_panics_doc,
    clippy::panic,
    clippy::must_use_candidate,
    clippy::unused_async,
    missing_debug_implementations,
    reason = "test fixtures abort the test on failure and are not a public API"
)]

mod clock;
mod community;
mod host;
mod launcher;
mod notes;
mod packages;

pub use clock::ManualClock;
pub use community::{Community, LOOPBACK, Member, Owner, free_port};
pub use host::{TestHost, start_host};
pub use launcher::InProcessLauncher;
pub use notes::{BackendHandles, NOTES_MIGRATIONS, NotesBackend, backend_launcher, notes_launcher};
pub use packages::{PackageSpec, build_package};
