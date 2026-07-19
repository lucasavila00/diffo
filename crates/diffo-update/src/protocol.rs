use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::{ErrorCategory, UpdateError};

pub const TARGET: &str = "x86_64-unknown-linux-gnu";
const ASSET_NAME: &str = "diffo-x86_64-unknown-linux-gnu";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Manifest {
    pub schema: u64,
    pub version: String,
    pub assets: Vec<Asset>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Asset {
    pub name: String,
    pub length: u64,
    pub target: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdatePlan {
    pub current: Version,
    pub version: Version,
    pub asset: Asset,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CheckOutcome {
    UpToDate { current: Version, latest: Version },
    Available(UpdatePlan),
}

pub(crate) fn verify_manifest(
    bytes: &[u8],
    signature: &[u8],
    public_key: &VerifyingKey,
    current: &str,
) -> Result<CheckOutcome, UpdateError> {
    let encoded = std::str::from_utf8(signature)
        .map_err(|_| verification("manifest signature is not UTF-8"))?;
    let signature = BASE64
        .decode(encoded.trim())
        .map_err(|_| verification("manifest signature is not valid base64"))?;
    let signature = Signature::from_slice(&signature)
        .map_err(|_| verification("manifest signature has the wrong length"))?;
    public_key
        .verify(bytes, &signature)
        .map_err(|_| verification("manifest signature is invalid"))?;

    let manifest: Manifest = serde_json::from_slice(bytes)
        .map_err(|error| verification(format!("manifest is invalid: {error}")))?;
    if manifest.schema != 1 {
        return Err(verification(format!(
            "unsupported update schema {}",
            manifest.schema
        )));
    }
    let current = stable_version(current, "running version")?;
    let latest = stable_version(&manifest.version, "release version")?;
    let asset = manifest
        .assets
        .into_iter()
        .find(|asset| asset.target == TARGET && asset.name == ASSET_NAME)
        .ok_or_else(|| verification(format!("release has no asset for {TARGET}")))?;
    validate_asset(&asset)?;
    if latest <= current {
        return Ok(CheckOutcome::UpToDate { current, latest });
    }
    Ok(CheckOutcome::Available(UpdatePlan {
        current,
        version: latest,
        asset,
    }))
}

fn stable_version(value: &str, label: &str) -> Result<Version, UpdateError> {
    let version = Version::parse(value)
        .map_err(|error| verification(format!("{label} is invalid: {error}")))?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err(verification(format!("{label} is not stable")));
    }
    Ok(version)
}

fn validate_asset(asset: &Asset) -> Result<(), UpdateError> {
    if asset.length == 0 {
        return Err(verification("release asset is empty"));
    }
    if asset.sha256.len() != 64
        || !asset
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(verification(
            "release asset SHA-256 must be 64 lowercase hexadecimal characters",
        ));
    }
    Ok(())
}

fn verification(message: impl Into<String>) -> UpdateError {
    UpdateError::new(ErrorCategory::Verification, message)
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signer as _, SigningKey};
    use serde_json::json;

    use super::*;

    const SEED: [u8; 32] = [
        0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c,
        0xc4, 0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae,
        0x7f, 0x60,
    ];

    fn signed(value: &serde_json::Value, current: &str) -> Result<CheckOutcome, UpdateError> {
        let bytes = serde_json::to_vec(&value).unwrap();
        let key = SigningKey::from_bytes(&SEED);
        let signature = BASE64.encode(key.sign(&bytes).to_bytes());
        verify_manifest(&bytes, signature.as_bytes(), &key.verifying_key(), current)
    }

    fn manifest(version: &str) -> serde_json::Value {
        json!({
            "schema": 1,
            "version": version,
            "assets": [{
                "name": ASSET_NAME,
                "length": 3,
                "target": TARGET,
                "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            }],
            "additive-field": {"is": "ignored"}
        })
    }

    #[test]
    fn accepts_additive_fields_and_selects_a_strictly_newer_stable_release() {
        let outcome = signed(&manifest("0.2.0"), "0.1.0").unwrap();
        assert!(matches!(outcome, CheckOutcome::Available(_)));
    }

    #[test]
    fn equal_and_older_releases_are_up_to_date() {
        assert!(matches!(
            signed(&manifest("0.1.0"), "0.1.0").unwrap(),
            CheckOutcome::UpToDate { .. }
        ));
        assert!(matches!(
            signed(&manifest("0.0.9"), "0.1.0").unwrap(),
            CheckOutcome::UpToDate { .. }
        ));
    }

    #[test]
    fn rejects_bad_signatures_before_parsing_metadata() {
        let key = SigningKey::from_bytes(&SEED);
        let signature = BASE64.encode(key.sign(b"different").to_bytes());
        let error = verify_manifest(
            b"not json",
            signature.as_bytes(),
            &key.verifying_key(),
            "0.1.0",
        )
        .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Verification);
        assert!(error.to_string().contains("signature"));
    }

    #[test]
    fn rejects_protocol_version_target_and_digest_mismatches() {
        let mut cases = Vec::new();
        let mut schema = manifest("0.2.0");
        schema["schema"] = json!(2);
        cases.push(schema);
        cases.push(manifest("0.2.0-alpha.1"));
        let mut target = manifest("0.2.0");
        target["assets"][0]["target"] = json!("aarch64-unknown-linux-gnu");
        cases.push(target);
        let mut digest = manifest("0.2.0");
        digest["assets"][0]["sha256"] = json!("ABC");
        cases.push(digest);

        for value in cases {
            assert_eq!(
                signed(&value, "0.1.0").unwrap_err().category(),
                ErrorCategory::Verification
            );
        }
    }
}
