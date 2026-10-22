use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ilmek_identity::PublisherSigner;
use ilmek_identity::storage::write_secret;
use serde::{Deserialize, Serialize};

use crate::cli::Publisher;
use crate::error::{CliError, CliResult, Context};
use crate::output::{line, note};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyFile {
    seed: String,
}

pub(crate) fn load_signer(path: &std::path::Path) -> CliResult<PublisherSigner> {
    let file: KeyFile =
        serde_json::from_slice(&std::fs::read(path).context("cannot read the key file")?)
            .context("key file is not valid")?;
    let seed: [u8; 32] = URL_SAFE_NO_PAD
        .decode(file.seed)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| CliError::new("key file seed is not 32 bytes"))?;
    Ok(PublisherSigner::from_seed(&seed))
}

pub(crate) fn run(command: Publisher) -> CliResult {
    match command {
        Publisher::Keygen { out } => {
            let signer = PublisherSigner::generate().context("cannot generate a key")?;
            let file = KeyFile {
                seed: URL_SAFE_NO_PAD.encode(signer.seed()),
            };
            let bytes = serde_json::to_vec(&file).context("cannot encode the key")?;
            write_secret(&out, &bytes)
                .context("cannot write the key file (it must not already exist)")?;
            note(&format!(
                "Signing key written to {} (owner-only). Back it up; it cannot be recovered.",
                out.display()
            ));
            line(&signer.public_key().to_base64url());
            Ok(())
        }
        Publisher::Show { key } => {
            let public = load_signer(&key)?.public_key();
            line(&format!("public key {}", public.to_base64url()));
            line(&format!("key id     {}", public.key_id()));
            Ok(())
        }
    }
}
