#![doc = include_str!("../README.md")]

mod install;
mod protocol;

use std::{env, fmt, io::Read as _, path::PathBuf};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::VerifyingKey;
use reqwest::{StatusCode, blocking::Client, redirect::Policy};

pub use install::{InstallOutcome, resolved_executable};
pub use protocol::{Asset, CheckOutcome, Manifest, UpdatePlan};

const DEFAULT_BASE_URL: &str = "https://github.com/lucasavila00/diffo/releases/latest/download";
const MANIFEST_NAME: &str = "update-v1.json";
const SIGNATURE_NAME: &str = "update-v1.json.sig";
const MAX_METADATA_BYTES: u64 = 1024 * 1024;
const MAX_SIGNATURE_BYTES: u64 = 1024;

// RFC 8032 test-vector key. Production release builds must replace this at compile
// time with DIFFO_UPDATE_PUBLIC_KEY; the release workflow enforces that contract.
const DEVELOPMENT_PUBLIC_KEY: [u8; 32] = [
    0xd7, 0x5a, 0x98, 0x01, 0x82, 0xb1, 0x0a, 0xb7, 0xd5, 0x4b, 0xfe, 0xd3, 0xc9, 0x64, 0x07, 0x3a,
    0x0e, 0xe1, 0x72, 0xf3, 0xda, 0xa6, 0x23, 0x25, 0xaf, 0x02, 0x1a, 0x68, 0xf7, 0x07, 0x51, 0x1a,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorCategory {
    Network,
    Verification,
    Permission,
    Other,
}

#[derive(Debug)]
pub struct UpdateError {
    category: ErrorCategory,
    message: String,
}

impl UpdateError {
    pub(crate) fn new(category: ErrorCategory, message: impl Into<String>) -> Self {
        Self {
            category,
            message: message.into(),
        }
    }

    pub(crate) fn io(action: &str, error: &std::io::Error) -> Self {
        let category = if error.kind() == std::io::ErrorKind::PermissionDenied {
            ErrorCategory::Permission
        } else {
            ErrorCategory::Other
        };
        Self::new(category, format!("{action}: {error}"))
    }

    #[must_use]
    pub const fn category(&self) -> ErrorCategory {
        self.category
    }
}

impl fmt::Display for UpdateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for UpdateError {}

pub struct UpdateClient {
    base_url: String,
    public_key: VerifyingKey,
    http: Client,
}

impl UpdateClient {
    /// Constructs the fixed updater, honoring developer-only endpoint and key hooks.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP client or test public key cannot be constructed.
    pub fn from_environment() -> Result<Self, UpdateError> {
        let base_url = env::var("DIFFO_UPDATE_BASE_URL").unwrap_or_else(|_| {
            option_env!("DIFFO_UPDATE_BASE_URL")
                .unwrap_or(DEFAULT_BASE_URL)
                .to_owned()
        });
        if env::var_os("DIFFO_UPDATE_BASE_URL").is_none() && !base_url.starts_with("https://") {
            return Err(UpdateError::new(
                ErrorCategory::Verification,
                "compiled update endpoint is not HTTPS",
            ));
        }
        let public_key = match environment_public_key()? {
            Some(public_key) => public_key,
            None => VerifyingKey::from_bytes(&DEVELOPMENT_PUBLIC_KEY).map_err(|_| {
                UpdateError::new(
                    ErrorCategory::Verification,
                    "compiled development update key is invalid",
                )
            })?,
        };
        let http = Client::builder()
            .user_agent(concat!("diffo/", env!("CARGO_PKG_VERSION")))
            .redirect(Policy::custom(|attempt| {
                if attempt.previous().len() >= 10 {
                    return attempt.stop();
                }
                let initial_scheme = attempt.previous().first().map_or("", |url| url.scheme());
                if redirect_is_secure(initial_scheme, attempt.url().scheme()) {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()
            .map_err(|error| UpdateError::new(ErrorCategory::Network, error.to_string()))?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            public_key,
            http,
        })
    }

    /// Fetches and verifies the latest schema-1 release metadata.
    ///
    /// # Errors
    ///
    /// Returns network or verification errors without changing the executable.
    pub fn check(&self) -> Result<CheckOutcome, UpdateError> {
        let manifest = self.fetch_limited(MANIFEST_NAME, MAX_METADATA_BYTES)?;
        let signature = self.fetch_limited(SIGNATURE_NAME, MAX_SIGNATURE_BYTES)?;
        protocol::verify_manifest(
            &manifest,
            &signature,
            &self.public_key,
            env!("CARGO_PKG_VERSION"),
        )
    }

    /// Downloads and installs the latest strictly newer verified release.
    ///
    /// # Errors
    ///
    /// Returns an error when discovery, verification, download, or replacement fails.
    pub fn install_latest(&self) -> Result<InstallOutcome, UpdateError> {
        let path = resolved_executable()?;
        match self.check()? {
            CheckOutcome::UpToDate { current, latest } => {
                Ok(InstallOutcome::UpToDate { current, latest })
            }
            CheckOutcome::Available(plan) => {
                let response = self.get(&plan.asset.name)?;
                install::install_response(&path, &plan, response)
            }
        }
    }

    fn get(&self, name: &str) -> Result<reqwest::blocking::Response, UpdateError> {
        let response = self
            .http
            .get(format!("{}/{name}", self.base_url))
            .send()
            .map_err(network_error)?;
        if response.status() != StatusCode::OK {
            return Err(UpdateError::new(
                ErrorCategory::Network,
                format!(
                    "update server returned HTTP {} for {name}",
                    response.status()
                ),
            ));
        }
        Ok(response)
    }

    fn fetch_limited(&self, name: &str, limit: u64) -> Result<Vec<u8>, UpdateError> {
        let mut response = self.get(name)?;
        if response
            .content_length()
            .is_some_and(|length| length > limit)
        {
            return Err(UpdateError::new(
                ErrorCategory::Verification,
                format!("{name} exceeds its size limit"),
            ));
        }
        let mut bytes = Vec::new();
        response
            .by_ref()
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(network_error)?;
        if bytes.len() as u64 > limit {
            return Err(UpdateError::new(
                ErrorCategory::Verification,
                format!("{name} exceeds its size limit"),
            ));
        }
        Ok(bytes)
    }
}

fn environment_public_key() -> Result<Option<VerifyingKey>, UpdateError> {
    let value = env::var("DIFFO_UPDATE_PUBLIC_KEY")
        .ok()
        .or_else(|| option_env!("DIFFO_UPDATE_PUBLIC_KEY").map(str::to_owned));
    let Some(value) = value else {
        return Ok(None);
    };
    let bytes = BASE64.decode(value.trim()).map_err(|_| {
        UpdateError::new(
            ErrorCategory::Verification,
            "update public key is not valid base64",
        )
    })?;
    let bytes: [u8; 32] = bytes.try_into().map_err(|_| {
        UpdateError::new(
            ErrorCategory::Verification,
            "update public key must contain 32 bytes",
        )
    })?;
    VerifyingKey::from_bytes(&bytes)
        .map(Some)
        .map_err(|_| UpdateError::new(ErrorCategory::Verification, "update public key is invalid"))
}

fn network_error(error: impl fmt::Display) -> UpdateError {
    UpdateError::new(ErrorCategory::Network, error.to_string())
}

fn redirect_is_secure(initial_scheme: &str, next_scheme: &str) -> bool {
    initial_scheme != "https" || next_scheme == "https"
}

/// Returns a shell-safe command for retrying a permission-denied update.
#[must_use]
pub fn sudo_command(path: &std::path::Path) -> String {
    let display = path.to_string_lossy();
    format!("sudo '{}' update", display.replace('\'', "'\\''"))
}

/// Resolves the running image for a permission-error hint without masking the
/// original update failure.
#[must_use]
pub fn permission_hint() -> Option<String> {
    resolved_executable()
        .ok()
        .map(|path: PathBuf| sudo_command(&path))
}

#[cfg(test)]
mod tests {
    use super::redirect_is_secure;

    #[test]
    fn secure_update_endpoints_never_redirect_to_an_insecure_scheme() {
        assert!(redirect_is_secure("https", "https"));
        assert!(!redirect_is_secure("https", "http"));
        assert!(redirect_is_secure("http", "http"), "local test hook");
    }
}
