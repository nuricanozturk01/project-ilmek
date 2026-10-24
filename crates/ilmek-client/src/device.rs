//! A device's persistent state: the community profile and a self-chosen device id.

use std::path::{Path, PathBuf};

use ilmek_core::community::{CommunityProfile, load_or_create_device_id};
use ilmek_protocol::Uuid7;

use crate::error::Error;

const PROFILE_FILE: &str = "profile.json";
const DEVICE_ID_FILE: &str = "device.id";

#[derive(Debug)]
pub struct Device {
    pub profile: CommunityProfile,
    /// Declared to servers in HELLO. Nothing proves it; it only tells devices apart.
    pub peer_id: Uuid7,
}

fn file(dir: &Path, name: &str) -> PathBuf {
    dir.join(name)
}

impl Device {
    /// Joins a community: stores the profile in `dir` and creates a device id on first use.
    pub fn join(profile: &CommunityProfile, dir: &Path) -> Result<Self, Error> {
        std::fs::create_dir_all(dir)?;
        profile.save(&file(dir, PROFILE_FILE))?;
        Self::load(dir)
    }

    pub fn load(dir: &Path) -> Result<Self, Error> {
        let profile = CommunityProfile::load(&file(dir, PROFILE_FILE))?;
        let peer_id = load_or_create_device_id(&file(dir, DEVICE_ID_FILE))?;
        Ok(Self { profile, peer_id })
    }

    #[must_use]
    pub fn exists(dir: &Path) -> bool {
        file(dir, PROFILE_FILE).exists()
    }
}
