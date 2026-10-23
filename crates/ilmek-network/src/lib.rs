//! Planning and applying the network setup of a Linux routing node.
//!
//! Building a [`Plan`] only reads the machine. [`apply`] needs root and an explicit decision,
//! records everything it changes in a snapshot, and [`rollback`] reverses it.

mod apply;
mod cidr;
mod command;
mod config;
mod error;
mod plan;
mod render;
mod system;

pub use apply::{
    Applied, ApplyOptions, Environment, Phase, SnapshotInfo, apply, rollback, snapshots,
};
pub use cidr::Cidr;
pub use command::Command;
pub use config::{
    Band, Dhcp, Firewall, NodeConfig, Reservation, Route, Transit, TransitMode, UserNetwork,
};
pub use error::{ApplyError, ConfigError};
pub use plan::{FileWrite, Plan, Step, build as plan};
pub use system::{LinuxProbe, Probe, ProcessRunner, Runner};
