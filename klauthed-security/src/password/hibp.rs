//! Have I Been Pwned "Pwned Passwords" breach check (`feature = "hibp"`).
//!
//! Uses the [k-anonymity range API]: the password is SHA-1 hashed locally and
//! only the first **5 hex characters** of the digest are sent. The server
//! returns every suffix (with a breach count) sharing that prefix, and the match
//! is done client-side — so the full hash never leaves the process and a password
//! can be checked against breach corpora without disclosing it.
//!
//! ```no_run
//! use klauthed_security::password::hibp::HibpClient;
//!
//! # async fn run() -> Result<(), klauthed_security::SecurityError> {
//! let hibp = HibpClient::new();
//! if hibp.is_pwned("correct horse battery staple").await? {
//!     // reject the password
//! }
//! # Ok(())
//! # }
//! ```
//!
//! [k-anonymity range API]: https://haveibeenpwned.com/API/v3#PwnedPasswords

use sha1::{Digest, Sha1};

use crate::error::SecurityError;

/// The public HIBP "Pwned Passwords" range API base URL.
const DEFAULT_BASE_URL: &str = "https://api.pwnedpasswords.com";

/// A client for the Have I Been Pwned "Pwned Passwords" range API.
#[derive(Debug, Clone)]
pub struct HibpClient {
    client: reqwest::Client,
    base_url: String,
}

impl Default for HibpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HibpClient {
    /// A client against the public HIBP API.
    #[must_use]
    pub fn new() -> Self {
        Self { client: reqwest::Client::new(), base_url: DEFAULT_BASE_URL.to_owned() }
    }

    /// Use a custom [`reqwest::Client`] (timeouts, proxy, …).
    #[must_use]
    pub fn with_client(mut self, client: reqwest::Client) -> Self {
        self.client = client;
        self
    }

    /// Point at a different base URL (a mirror, or a test server).
    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into().trim_end_matches('/').to_owned();
        self
    }

    /// How many times `password` appears in known breaches (`0` = not found).
    ///
    /// Only the first five hex chars of the password's SHA-1 are sent.
    ///
    /// # Errors
    /// Returns [`SecurityError::Hibp`] if the API can't be reached or returns a
    /// non-success status.
    pub async fn pwned_count(&self, password: &str) -> Result<u64, SecurityError> {
        let digest = Sha1::digest(password.as_bytes());
        let hash = hex::encode_upper(digest);
        let (prefix, suffix) = hash.split_at(5);

        let response = self
            .client
            .get(format!("{}/range/{prefix}", self.base_url))
            // Padding hides the real result-set size from a network observer.
            .header("Add-Padding", "true")
            .send()
            .await
            .map_err(|e| SecurityError::Hibp(e.to_string()))?;

        if !response.status().is_success() {
            return Err(SecurityError::Hibp(format!("range API returned {}", response.status())));
        }

        let body = response.text().await.map_err(|e| SecurityError::Hibp(e.to_string()))?;
        Ok(parse_count(&body, suffix))
    }

    /// Whether `password` appears in any known breach.
    ///
    /// # Errors
    /// See [`pwned_count`](Self::pwned_count).
    pub async fn is_pwned(&self, password: &str) -> Result<bool, SecurityError> {
        Ok(self.pwned_count(password).await? > 0)
    }
}

/// Find `suffix` among a range response's `SUFFIX:COUNT` lines and return its
/// count, or `0` if absent. Padding rows (count `0`) fall through naturally.
fn parse_count(body: &str, suffix: &str) -> u64 {
    for line in body.lines() {
        let Some((candidate, count)) = line.trim().split_once(':') else {
            continue;
        };
        if candidate.eq_ignore_ascii_case(suffix) {
            return count.trim().parse().unwrap_or(0);
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // SHA-1("password") = 5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8
    const PASSWORD_PREFIX: &str = "5BAA6";
    const PASSWORD_SUFFIX: &str = "1E4C9B93F3F0682250B6CF8331B7EE68FD8";

    #[test]
    fn parse_count_matches_case_insensitively_and_defaults_to_zero() {
        let body = "0000000000000000000000000000000000A:10\r\n\
                    1e4c9b93f3f0682250b6cf8331b7ee68fd8:42\r\n";
        assert_eq!(parse_count(body, PASSWORD_SUFFIX), 42);
        assert_eq!(parse_count(body, "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF"), 0);
    }

    #[tokio::test]
    async fn reports_a_breached_password() {
        let server = MockServer::start().await;
        let body = format!("0000000000000000000000000000000000A:10\r\n{PASSWORD_SUFFIX}:42\r\n");
        Mock::given(method("GET"))
            .and(path(format!("/range/{PASSWORD_PREFIX}")))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;

        let hibp = HibpClient::new().with_base_url(server.uri());
        assert_eq!(hibp.pwned_count("password").await.unwrap(), 42);
        assert!(hibp.is_pwned("password").await.unwrap());
    }

    #[tokio::test]
    async fn reports_a_clean_password() {
        let server = MockServer::start().await;
        // Response for the prefix that does not contain our suffix.
        Mock::given(method("GET"))
            .and(path(format!("/range/{PASSWORD_PREFIX}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string("0000000000000000000000000000000000A:10\r\n"),
            )
            .mount(&server)
            .await;

        let hibp = HibpClient::new().with_base_url(server.uri());
        assert_eq!(hibp.pwned_count("password").await.unwrap(), 0);
        assert!(!hibp.is_pwned("password").await.unwrap());
    }

    #[tokio::test]
    async fn maps_a_server_error_to_hibp() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).respond_with(ResponseTemplate::new(503)).mount(&server).await;

        let err = HibpClient::new().with_base_url(server.uri()).pwned_count("password").await;
        assert!(matches!(err, Err(SecurityError::Hibp(_))));
    }
}
