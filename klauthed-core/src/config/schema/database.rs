use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Supported database systems — relational and NoSQL.
///
/// This is config-layer metadata only; it tells downstream crates which driver
/// to use and how to shape a connection string. Adding a system here does not by
/// itself wire a pool — that lives in `klauthed-data`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DbSystem {
    #[default]
    Postgres,
    #[serde(rename = "mysql")]
    MySql,
    #[serde(rename = "mariadb")]
    MariaDb,
    Mssql,
    Sqlite,
    /// NoSQL document store.
    #[serde(rename = "mongodb")]
    MongoDb,
}

impl DbSystem {
    /// The conventional default port, or `None` for file-based engines (SQLite).
    pub fn default_port(&self) -> Option<u16> {
        match self {
            DbSystem::Postgres => Some(5432),
            DbSystem::MySql | DbSystem::MariaDb => Some(3306),
            DbSystem::Mssql => Some(1433),
            DbSystem::MongoDb => Some(27017),
            DbSystem::Sqlite => None,
        }
    }

    /// The URL scheme used in a connection string.
    pub fn scheme(&self) -> &'static str {
        match self {
            DbSystem::Postgres => "postgres",
            DbSystem::MySql | DbSystem::MariaDb => "mysql",
            DbSystem::Mssql => "sqlserver",
            DbSystem::Sqlite => "sqlite",
            DbSystem::MongoDb => "mongodb",
        }
    }

    /// Whether this system is relational (vs. a document/NoSQL store).
    pub fn is_relational(&self) -> bool {
        !matches!(self, DbSystem::MongoDb)
    }
}

/// Connection-pool tuning shared by database and cache configs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolConfig {
    /// Maximum number of connections in the pool.
    #[serde(default = "PoolConfig::default_max_connections")]
    pub max_connections: u32,
    /// Minimum number of idle connections kept warm.
    #[serde(default)]
    pub min_connections: u32,
    /// How long to wait for a connection before erroring.
    #[serde(default = "PoolConfig::default_acquire_timeout_secs")]
    pub acquire_timeout_secs: u64,
    /// Close a connection after it has been idle this long (`None` = never).
    #[serde(default)]
    pub idle_timeout_secs: Option<u64>,
    /// Recycle a connection after this total lifetime (`None` = never).
    #[serde(default)]
    pub max_lifetime_secs: Option<u64>,
}

impl PoolConfig {
    fn default_max_connections() -> u32 {
        10
    }
    fn default_acquire_timeout_secs() -> u64 {
        30
    }
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_connections: Self::default_max_connections(),
            min_connections: 0,
            acquire_timeout_secs: Self::default_acquire_timeout_secs(),
            idle_timeout_secs: None,
            max_lifetime_secs: None,
        }
    }
}

/// Connection details for a database, expressed either as components
/// (host/port/credentials) or a full `url` that overrides them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DatabaseConfig {
    /// Which database system this describes.
    #[serde(default)]
    pub system: DbSystem,
    /// Hostname (ignored when `url` is set or `system` is SQLite).
    #[serde(default = "default_host")]
    pub host: String,
    /// Port; falls back to [`DbSystem::default_port`] when unset.
    #[serde(default)]
    pub port: Option<u16>,
    /// Database / catalog name (or file path for SQLite).
    #[serde(default)]
    pub database: String,
    #[serde(default)]
    pub username: Option<String>,
    /// Password. Prefer sourcing this from Vault in staging/prod.
    #[serde(default)]
    pub password: Option<String>,
    /// A complete connection URL. When present it is used verbatim and the
    /// component fields above are ignored.
    #[serde(default)]
    pub url: Option<String>,
    /// Extra connection parameters appended as a query string (e.g. `sslmode`,
    /// `replicaSet`).
    #[serde(default)]
    pub options: BTreeMap<String, String>,
    /// Pool tuning.
    #[serde(default)]
    pub pool: PoolConfig,
}

fn default_host() -> String {
    "localhost".to_owned()
}

impl DatabaseConfig {
    /// The effective port: explicit `port`, else the system default.
    pub fn effective_port(&self) -> Option<u16> {
        self.port.or_else(|| self.system.default_port())
    }

    /// Build a connection URL from the components, or return `url` verbatim if set.
    ///
    /// Note: credentials are inserted as-is and not percent-encoded, so a
    /// password containing URL-reserved characters should be supplied via the
    /// pre-built `url` field instead.
    pub fn connection_url(&self) -> String {
        if let Some(url) = &self.url {
            return url.clone();
        }

        let scheme = self.system.scheme();
        if self.system == DbSystem::Sqlite {
            return format!("{scheme}://{}", self.database);
        }

        let mut url = format!("{scheme}://");
        if let Some(user) = &self.username {
            url.push_str(user);
            if let Some(password) = &self.password {
                url.push(':');
                url.push_str(password);
            }
            url.push('@');
        }
        url.push_str(&self.host);
        if let Some(port) = self.effective_port() {
            url.push(':');
            url.push_str(&port.to_string());
        }
        url.push('/');
        url.push_str(&self.database);

        if !self.options.is_empty() {
            let query =
                self.options.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&");
            url.push('?');
            url.push_str(&query);
        }
        url
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deserializes_with_defaults() {
        let cfg: DatabaseConfig = serde_json::from_value(json!({
            "system": "postgres",
            "database": "app",
            "username": "svc",
            "password": "pw"
        }))
        .unwrap();

        assert_eq!(cfg.host, "localhost");
        assert_eq!(cfg.effective_port(), Some(5432));
        assert_eq!(cfg.pool.max_connections, 10);
        assert_eq!(cfg.connection_url(), "postgres://svc:pw@localhost:5432/app");
    }

    #[test]
    fn url_field_overrides_components() {
        let cfg = DatabaseConfig { url: Some("postgres://custom/db".into()), ..Default::default() };
        assert_eq!(cfg.connection_url(), "postgres://custom/db");
    }

    #[test]
    fn mongodb_url_with_options() {
        let mut options = BTreeMap::new();
        options.insert("replicaSet".to_string(), "rs0".to_string());
        let cfg = DatabaseConfig {
            system: DbSystem::MongoDb,
            host: "mongo".into(),
            database: "app".into(),
            options,
            ..Default::default()
        };
        assert_eq!(cfg.effective_port(), Some(27017));
        assert!(!cfg.system.is_relational());
        assert_eq!(cfg.connection_url(), "mongodb://mongo:27017/app?replicaSet=rs0");
    }

    #[test]
    fn db_system_external_names_are_natural() {
        let cases = [
            ("postgres", DbSystem::Postgres),
            ("mysql", DbSystem::MySql),
            ("mariadb", DbSystem::MariaDb),
            ("mssql", DbSystem::Mssql),
            ("sqlite", DbSystem::Sqlite),
            ("mongodb", DbSystem::MongoDb),
        ];
        for (name, expected) in cases {
            let parsed: DbSystem = serde_json::from_value(json!(name)).unwrap();
            assert_eq!(parsed, expected, "deserializing {name}");
            assert_eq!(serde_json::to_value(expected).unwrap(), json!(name));
        }
    }

    #[test]
    fn sqlite_uses_path() {
        let cfg = DatabaseConfig {
            system: DbSystem::Sqlite,
            database: "/var/lib/app.db".into(),
            ..Default::default()
        };
        assert_eq!(cfg.effective_port(), None);
        assert_eq!(cfg.connection_url(), "sqlite:///var/lib/app.db");
    }
}
