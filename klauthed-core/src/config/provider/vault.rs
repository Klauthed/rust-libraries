//! HashiCorp Vault configuration provider (KV v2).
//!
//! This is a small, purpose-built Vault client wired over `reqwest` — we
//! deliberately do not depend on `vaultrs`. It supports the three auth methods
//! a backend service realistically needs:
//!
//! * **Token** — a pre-issued token (`VAULT_TOKEN`), typical for local/dev.
//! * **AppRole** — `role_id` + `secret_id` login, the standard for services.
//! * **Kubernetes** — service-account JWT login, for pod-identity in k8s.
//!
//! Secrets can be wired explicitly (`with_secret(key, path)`) or discovered
//! under a base path via KV `LIST`. Each secret's KV data object is mounted at
//! its config key (dotted keys nest), then deep-merged into the config tree like
//! any other provider.
//!
//! Only available with the `vault` feature enabled.

use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use serde_json::{Map, Value};

use super::{ConfigProvider, ProviderKind};
use crate::config::map::ConfigMap;
use crate::error::ConfigError;

/// How the provider authenticates to Vault.
#[derive(Clone)]
pub enum VaultAuth {
    /// A pre-issued client token.
    Token(SecretString),

    /// AppRole login (`auth/{mount}/login`).
    AppRole {
        /// The AppRole role identifier.
        role_id: String,
        /// The AppRole secret identifier.
        secret_id: SecretString,
        /// Auth mount path, usually `"approle"`.
        mount: String,
    },

    /// Kubernetes login (`auth/{mount}/login`) using a service-account JWT.
    Kubernetes {
        /// The Vault role to authenticate as.
        role: String,
        /// The service-account JWT presented to Vault.
        jwt: SecretString,
        /// Auth mount path, usually `"kubernetes"`.
        mount: String,
    },
}

impl VaultAuth {
    fn method_name(&self) -> &'static str {
        match self {
            VaultAuth::Token(_) => "token",
            VaultAuth::AppRole { .. } => "approle",
            VaultAuth::Kubernetes { .. } => "kubernetes",
        }
    }
}

/// A single explicit secret-to-config-key binding.
#[derive(Clone)]
struct SecretMapping {
    /// Config key the secret's KV data is mounted under (may be dotted to nest).
    key: String,
    /// KV v2 path relative to the mount, e.g. `"myapp/database"`.
    path: String,
}

/// Reads configuration secrets from HashiCorp Vault's KV v2 engine.
pub struct VaultProvider {
    address: String,
    namespace: Option<String>,
    kv_mount: String,
    auth: VaultAuth,
    secrets: Vec<SecretMapping>,
    base_path: Option<String>,
    client: reqwest::Client,
}

impl VaultProvider {
    /// Create a provider for `address` (e.g. `https://vault.internal:8200`) using `auth`.
    /// Defaults: KV mount `"secret"`, no namespace, no secrets wired yet.
    pub fn new(address: impl Into<String>, auth: VaultAuth) -> Self {
        Self {
            address: address.into().trim_end_matches('/').to_owned(),
            namespace: None,
            kv_mount: "secret".to_owned(),
            auth,
            secrets: Vec::new(),
            base_path: None,
            client: reqwest::Client::new(),
        }
    }

    /// Set the KV v2 mount path (default `"secret"`).
    #[must_use]
    pub fn kv_mount(mut self, mount: impl Into<String>) -> Self {
        self.kv_mount = mount.into();
        self
    }

    /// Set the Vault Enterprise namespace (`X-Vault-Namespace`).
    #[must_use]
    pub fn namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = Some(namespace.into());
        self
    }

    /// Mount the KV data at `path` under config `key` (dotted keys nest).
    #[must_use]
    pub fn with_secret(mut self, key: impl Into<String>, path: impl Into<String>) -> Self {
        self.secrets.push(SecretMapping { key: key.into(), path: path.into() });
        self
    }

    /// Discover and read every secret beneath `base_path` via KV `LIST`,
    /// mounting each under a config key derived from its path relative to
    /// `base_path` (slashes become dots).
    #[must_use]
    pub fn with_base_path(mut self, base_path: impl Into<String>) -> Self {
        self.base_path = Some(base_path.into().trim_matches('/').to_owned());
        self
    }

    /// Build a provider entirely from the environment.
    ///
    /// * `VAULT_ADDR` (required)
    /// * `VAULT_NAMESPACE`, `VAULT_KV_MOUNT` (optional; mount defaults to `secret`)
    /// * `APP_VAULT_PATH` / `VAULT_KV_PATH` (optional base path for discovery)
    /// * Auth, in precedence order:
    ///   * `VAULT_TOKEN` → Token
    ///   * `VAULT_ROLE_ID` + `VAULT_SECRET_ID` → AppRole (`VAULT_APPROLE_MOUNT`, default `approle`)
    ///   * `VAULT_K8S_ROLE` → Kubernetes (JWT from `VAULT_K8S_JWT_PATH`, default the
    ///     standard service-account token path; `VAULT_K8S_MOUNT`, default `kubernetes`)
    pub fn from_env() -> Result<Self, ConfigError> {
        let address = std::env::var("VAULT_ADDR")
            .map_err(|_| ConfigError::MissingEnv("VAULT_ADDR".into()))?;

        let auth = Self::auth_from_env()?;
        let mut provider = Self::new(address, auth);

        if let Ok(ns) = std::env::var("VAULT_NAMESPACE") {
            provider = provider.namespace(ns);
        }
        if let Ok(mount) = std::env::var("VAULT_KV_MOUNT") {
            provider = provider.kv_mount(mount);
        }
        if let Ok(base) =
            std::env::var("APP_VAULT_PATH").or_else(|_| std::env::var("VAULT_KV_PATH"))
        {
            provider = provider.with_base_path(base);
        }

        Ok(provider)
    }

    fn auth_from_env() -> Result<VaultAuth, ConfigError> {
        if let Ok(token) = std::env::var("VAULT_TOKEN") {
            return Ok(VaultAuth::Token(SecretString::from(token)));
        }

        if let (Ok(role_id), Ok(secret_id)) =
            (std::env::var("VAULT_ROLE_ID"), std::env::var("VAULT_SECRET_ID"))
        {
            return Ok(VaultAuth::AppRole {
                role_id,
                secret_id: SecretString::from(secret_id),
                mount: std::env::var("VAULT_APPROLE_MOUNT").unwrap_or_else(|_| "approle".into()),
            });
        }

        if let Ok(role) = std::env::var("VAULT_K8S_ROLE") {
            let jwt_path = std::env::var("VAULT_K8S_JWT_PATH")
                .unwrap_or_else(|_| "/var/run/secrets/kubernetes.io/serviceaccount/token".into());
            let jwt = std::fs::read_to_string(&jwt_path).map_err(|e| ConfigError::VaultAuth {
                method: "kubernetes".into(),
                message: format!("could not read service-account JWT at '{jwt_path}': {e}"),
            })?;
            return Ok(VaultAuth::Kubernetes {
                role,
                jwt: SecretString::from(jwt.trim().to_owned()),
                mount: std::env::var("VAULT_K8S_MOUNT").unwrap_or_else(|_| "kubernetes".into()),
            });
        }

        Err(ConfigError::VaultAuth {
            method: "auto".into(),
            message: "no Vault auth configured (set VAULT_TOKEN, VAULT_ROLE_ID + \
                      VAULT_SECRET_ID, or VAULT_K8S_ROLE)"
                .into(),
        })
    }

    fn request(
        &self,
        method: reqwest::Method,
        url: &str,
        token: Option<&str>,
    ) -> reqwest::RequestBuilder {
        let mut req = self.client.request(method, url);
        if let Some(token) = token {
            req = req.header("X-Vault-Token", token);
        }
        if let Some(ns) = &self.namespace {
            req = req.header("X-Vault-Namespace", ns);
        }
        req
    }

    /// Authenticate and return a usable client token.
    async fn login(&self) -> Result<SecretString, ConfigError> {
        match &self.auth {
            VaultAuth::Token(token) => Ok(token.clone()),

            VaultAuth::AppRole { role_id, secret_id, mount } => {
                let url = format!("{}/v1/auth/{}/login", self.address, mount);
                let body = serde_json::json!({
                    "role_id": role_id,
                    "secret_id": secret_id.expose_secret(),
                });
                self.login_request(&url, &body).await
            }

            VaultAuth::Kubernetes { role, jwt, mount } => {
                let url = format!("{}/v1/auth/{}/login", self.address, mount);
                let body = serde_json::json!({
                    "role": role,
                    "jwt": jwt.expose_secret(),
                });
                self.login_request(&url, &body).await
            }
        }
    }

    async fn login_request(&self, url: &str, body: &Value) -> Result<SecretString, ConfigError> {
        let method = self.auth.method_name();
        let resp = self.request(reqwest::Method::POST, url, None).json(body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let detail = resp.text().await.unwrap_or_default();
            return Err(ConfigError::VaultAuth {
                method: method.to_owned(),
                message: format!("login returned HTTP {status}: {detail}"),
            });
        }

        let parsed: AuthResponse = resp.json().await?;
        Ok(SecretString::from(parsed.auth.client_token))
    }

    /// Read a single KV v2 secret's data object.
    async fn read_secret(
        &self,
        token: &str,
        path: &str,
    ) -> Result<Map<String, Value>, ConfigError> {
        let url = format!("{}/v1/{}/data/{}", self.address, self.kv_mount, path);
        let resp = self.request(reqwest::Method::GET, &url, Some(token)).send().await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ConfigError::VaultSecretNotFound(path.to_owned()));
        }
        if !resp.status().is_success() {
            return Err(ConfigError::VaultRequest {
                path: path.to_owned(),
                message: format!("HTTP {}", resp.status()),
            });
        }

        let parsed: KvReadResponse = resp.json().await?;
        Ok(parsed.data.data)
    }

    /// Discover all leaf secret paths under `base_path` using iterative KV LIST
    /// (no async recursion — a work stack of folder prefixes).
    async fn discover(&self, token: &str, base_path: &str) -> Result<Vec<String>, ConfigError> {
        let mut leaves = Vec::new();
        let mut stack = vec![base_path.to_owned()];

        while let Some(prefix) = stack.pop() {
            let url = format!("{}/v1/{}/metadata/{}", self.address, self.kv_mount, prefix);
            let resp = self
                .request(reqwest::Method::GET, &url, Some(token))
                .query(&[("list", "true")])
                .send()
                .await?;

            if resp.status() == reqwest::StatusCode::NOT_FOUND {
                continue;
            }
            if !resp.status().is_success() {
                return Err(ConfigError::VaultRequest {
                    path: prefix.clone(),
                    message: format!("LIST returned HTTP {}", resp.status()),
                });
            }

            let parsed: ListResponse = resp.json().await?;
            for entry in parsed.data.keys {
                let child =
                    format!("{}/{}", prefix.trim_end_matches('/'), entry.trim_end_matches('/'));
                if entry.ends_with('/') {
                    stack.push(child);
                } else {
                    leaves.push(child);
                }
            }
        }

        Ok(leaves)
    }

    /// Turn an absolute leaf path into a config key relative to `base_path`,
    /// with slashes mapped to dots (`myapp/db/primary` → `db.primary`).
    fn key_for(base_path: &str, leaf: &str) -> String {
        leaf.strip_prefix(base_path).unwrap_or(leaf).trim_matches('/').replace('/', ".")
    }
}

#[async_trait::async_trait]
impl ConfigProvider for VaultProvider {
    fn name(&self) -> String {
        format!("vault:{}", self.address)
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::Vault
    }

    async fn load(&self) -> Result<ConfigMap, ConfigError> {
        let token = self.login().await?;
        let token = token.expose_secret();

        let mut flat = ConfigMap::new();

        for mapping in &self.secrets {
            let data = self.read_secret(token, &mapping.path).await?;
            flat.insert(mapping.key.clone(), Value::Object(data));
        }

        if let Some(base) = &self.base_path {
            for leaf in self.discover(token, base).await? {
                let key = Self::key_for(base, &leaf);
                if key.is_empty() {
                    continue;
                }
                let data = self.read_secret(token, &leaf).await?;
                flat.insert(key, Value::Object(data));
            }
        }

        tracing::debug!(
            address = %self.address,
            secrets = flat.len(),
            "loaded configuration from Vault"
        );

        Ok(flat.expand_dotted())
    }
}

// ── Vault API response shapes ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct AuthResponse {
    auth: AuthInfo,
}

#[derive(Deserialize)]
struct AuthInfo {
    client_token: String,
}

#[derive(Deserialize)]
struct KvReadResponse {
    data: KvData,
}

#[derive(Deserialize)]
struct KvData {
    data: Map<String, Value>,
}

#[derive(Deserialize)]
struct ListResponse {
    data: ListData,
}

#[derive(Deserialize)]
struct ListData {
    keys: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_for_strips_base_and_dots_slashes() {
        assert_eq!(VaultProvider::key_for("myapp", "myapp/database"), "database");
        assert_eq!(VaultProvider::key_for("myapp", "myapp/db/primary"), "db.primary");
    }

    #[test]
    fn new_normalizes_address_and_defaults_mount() {
        let p = VaultProvider::new(
            "https://vault.internal:8200/",
            VaultAuth::Token(SecretString::from("t".to_owned())),
        );
        assert_eq!(p.address, "https://vault.internal:8200");
        assert_eq!(p.kv_mount, "secret");
        assert_eq!(p.auth.method_name(), "token");
    }
}
