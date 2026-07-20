#![doc = include_str!("../README.md")]

mod install;
mod protocol;

use std::{env, fmt, io::Read as _, path::PathBuf};

use reqwest::{StatusCode, blocking::Client, redirect::Policy};

pub use install::{InstallOutcome, resolved_executable};
pub use protocol::{Asset, CheckOutcome, Manifest, UpdatePlan};

const DEFAULT_BASE_URL: &str = "https://raw.githubusercontent.com/lucasavila00/diffo/release";
const MANIFEST_NAME: &str = "update-v1.json";
const MAX_METADATA_BYTES: u64 = 1024 * 1024;
const BUILD_VERSION: &str = selected_build_version(option_env!("DIFFO_RELEASE_VERSION"));

const fn selected_build_version(release_version: Option<&'static str>) -> &'static str {
    match release_version {
        Some(version) => version,
        None => env!("CARGO_PKG_VERSION"),
    }
}

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
    http: Client,
}

impl UpdateClient {
    /// Constructs the fixed updater, honoring the developer-only endpoint hook.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP client cannot be constructed.
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
        let http = Client::builder()
            .user_agent(format!("diffo/{BUILD_VERSION}"))
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
        protocol::parse_manifest(&manifest, BUILD_VERSION)
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
    use super::{redirect_is_secure, selected_build_version};

    #[test]
    fn release_version_overrides_cargo_package_version() {
        assert_eq!(selected_build_version(Some("9.8.7")), "9.8.7");
        assert_eq!(selected_build_version(None), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn secure_update_endpoints_never_redirect_to_an_insecure_scheme() {
        assert!(redirect_is_secure("https", "https"));
        assert!(!redirect_is_secure("https", "http"));
        assert!(redirect_is_secure("http", "http"), "local test hook");
    }
}
