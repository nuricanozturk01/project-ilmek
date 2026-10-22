use std::io::Read;

use ilmek_client::{Device, enroll};
use ilmek_core::clock::{Clock, SystemClock};
use ilmek_identity::CommunityProfile;

use crate::cli::Identity;
use crate::error::{CliResult, Context};
use crate::output::{line, note};

const SEVEN_DAYS_MS: u64 = 7 * 24 * 60 * 60 * 1000;

pub(crate) async fn run(command: Identity) -> CliResult {
    match command {
        Identity::Enroll {
            profile,
            fingerprint,
            device,
            name,
            token_file,
        } => {
            let profile_bytes = std::fs::read(&profile).context("cannot read the profile")?;
            let profile = CommunityProfile::from_json(&profile_bytes).context("invalid profile")?;
            profile
                .verify_fingerprint(&fingerprint)
                .context("refusing to trust this profile")?;

            let token = read_token(token_file.as_deref())?;
            let enrolled = enroll(&profile, token.trim(), &name, &device)
                .await
                .context("enrollment failed")?;
            line(&format!("peer {}", enrolled.peer_id));
            note(&format!(
                "Identity stored in {}. Keep that directory private.",
                device.display()
            ));
            Ok(())
        }
        Identity::Show { device } => {
            let device = Device::load(&device).context("cannot load the device")?;
            let roles: Vec<String> = device
                .roles
                .iter()
                .map(|r| format!("{r:?}").to_lowercase())
                .collect();
            line(&format!("peer     {}", device.peer_id));
            line(&format!("roles    {}", roles.join(",")));
            line(&format!("registry {}", device.profile.registry_control));
            line(&format!("expires  {} ms since epoch", device.expires_at_ms));
            let remaining = device.expires_at_ms.saturating_sub(SystemClock.now_ms());
            if remaining == 0 {
                note("The certificate has expired. Ask the administrator for a renewal token.");
            } else if remaining < SEVEN_DAYS_MS {
                note(
                    "The certificate expires within 7 days. Ask the administrator for a renewal token.",
                );
            }
            Ok(())
        }
    }
}

fn read_token(path: Option<&std::path::Path>) -> CliResult<String> {
    if let Some(path) = path {
        return std::fs::read_to_string(path).context("cannot read the token file");
    }
    note("Paste the enrollment token and press Enter (it is not echoed to history or arguments):");
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .context("cannot read stdin")?;
    Ok(text)
}
