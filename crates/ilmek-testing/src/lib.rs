//! Test fixtures. Production code never depends on this crate.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "fixtures abort the test on failure"
)]

mod clock;
mod community;
mod host;
mod launcher;
mod notes;
mod packages;

pub use clock::ManualClock;
pub use community::{Community, LOOPBACK, Member, Owner, free_port, invite};
pub use host::{TestHost, start_host};
pub use launcher::InProcessLauncher;
pub use notes::{BackendHandles, NOTES_MIGRATIONS, NotesBackend, notes_launcher};
pub use packages::{PackageSpec, build_package};
