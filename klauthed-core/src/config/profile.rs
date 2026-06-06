use std::fmt;
use std::str::FromStr;

/// Defines the configuration profile for the application.
/// Profiles can be used to differentiate between environments (e.g., local, dev, staging, prod).
/// The profile can influence which config sources are used (e.g., Vault for prod/staging) and which security policies apply.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum Profile {
    /// Local developer machine. File secrets and env vars are allowed.
    #[default]
    Local,

    /// Shared development environment. File secrets and env vars are allowed.
    Dev,

    /// Automated test runs. File secrets and env vars are allowed.
    Test,

    /// Staging environment. Secrets must come from Vault.
    Staging,

    /// Production environment. Secrets must come from Vault.
    Prod,
}

impl Profile {
    /// Detect the profile from the `APP_PROFILE` or `KLAUTHED_PROFILE` env var.
    /// Falls back to [`Profile::Local`] if neither is set or the value is unrecognized.
    pub fn detect() -> Self {
        std::env::var("APP_PROFILE")
            .or_else(|_| std::env::var("KLAUTHED_PROFILE"))
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_default()
    }

    /// True for profiles where secrets must be sourced from Vault.
    pub fn requires_vault(&self) -> bool {
        matches!(self, Profile::Staging | Profile::Prod)
    }

    /// True for profiles where file/env secrets are permitted.
    pub fn allows_file_secrets(&self) -> bool {
        !self.requires_vault()
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Profile::Local => "local",
            Profile::Dev => "dev",
            Profile::Test => "test",
            Profile::Staging => "staging",
            Profile::Prod => "prod",
        }
    }
}

impl fmt::Display for Profile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Profile {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "local" => Ok(Profile::Local),
            "dev" => Ok(Profile::Dev),
            "test" => Ok(Profile::Test),
            "staging" => Ok(Profile::Staging),
            "prod" | "production" => Ok(Profile::Prod),
            _ => Err(()),
        }
    }
}
