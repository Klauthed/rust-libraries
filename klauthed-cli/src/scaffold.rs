//! Service scaffolding: render the embedded templates into a new project tree.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

// Templates embedded at compile time. They carry `// __IF flag__` / `// __END
// flag__` conditional blocks (see `apply_conditionals`) plus `__NAME__`-style
// placeholders, rendered by [`render`].
const CARGO_TMPL: &str = include_str!("../templates/Cargo.toml.tmpl");
const MAIN_TMPL: &str = include_str!("../templates/main.rs.tmpl");
const CONFIG_TMPL: &str = include_str!("../templates/config.default.toml.tmpl");
const GITIGNORE_TMPL: &str = include_str!("../templates/gitignore.tmpl");
const README_TMPL: &str = include_str!("../templates/README.md.tmpl");

/// Something that prevented a service from being scaffolded.
#[derive(Debug)]
pub enum ScaffoldError {
    /// The service name is not a valid crate name.
    InvalidName(String),
    /// The target directory already exists and is not empty.
    TargetExists(PathBuf),
    /// A filesystem operation failed.
    Io {
        /// The path being operated on when the error occurred.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
}

impl fmt::Display for ScaffoldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName(name) => write!(
                f,
                "'{name}' is not a valid service name: use letters, digits, '-' or '_', \
                 starting with a letter"
            ),
            Self::TargetExists(path) => {
                write!(f, "target directory '{}' already exists and is not empty", path.display())
            }
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for ScaffoldError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// The relational backend a scaffolded service is wired for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Database {
    /// PostgreSQL.
    Postgres,
    /// MySQL / MariaDB.
    Mysql,
    /// SQLite.
    Sqlite,
}

impl Database {
    /// The `klauthed` cargo feature that enables this backend (also pulls in
    /// `data` and, alongside `web`, the web `data-sql` integration).
    fn feature(self) -> &'static str {
        match self {
            Self::Postgres => "postgres",
            Self::Mysql => "mysql",
            Self::Sqlite => "sqlite",
        }
    }

    /// The `[database] system` value for `config/default.toml`.
    fn system(self) -> &'static str {
        match self {
            Self::Postgres => "postgres",
            Self::Mysql => "mysql",
            Self::Sqlite => "sqlite",
        }
    }

    /// A sample connection URL for `config/default.toml`.
    fn sample_url(self, name: &str) -> String {
        match self {
            Self::Postgres => format!("postgres://localhost:5432/{name}"),
            Self::Mysql => format!("mysql://localhost:3306/{name}"),
            Self::Sqlite => format!("sqlite://{name}.db?mode=rwc"),
        }
    }
}

/// Options controlling what a scaffolded service includes.
#[derive(Debug, Default, Clone, Copy)]
pub struct Options {
    /// Include JWT authentication: the `security` feature plus `/login` and a
    /// JWT-protected `/api/me` route.
    pub with_jwt: bool,
    /// Wire a relational connection pool into the web layer for this backend.
    pub database: Option<Database>,
    /// Start an interval scheduler with an example recurring task (enables the
    /// `scheduler` feature).
    pub with_scheduler: bool,
}

/// The `klauthed` features the generated project enables, as a TOML array body
/// (e.g. `"core", "web", "observability"`).
fn feature_list(options: &Options) -> String {
    let mut features = vec!["core", "web", "observability"];
    if options.with_jwt {
        features.push("security");
    }
    if let Some(db) = options.database {
        features.push(db.feature());
    }
    if options.with_scheduler {
        features.push("scheduler");
    }
    features.iter().map(|f| format!("\"{f}\"")).collect::<Vec<_>>().join(", ")
}

/// The `major.minor` klauthed version requirement that generated projects depend
/// on. Derived from this CLI's own version, since the whole suite shares one
/// version and ships together (CLI `0.6.x` scaffolds against `klauthed = "0.6"`).
fn klauthed_req() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let mut parts = version.split('.');
    match (parts.next(), parts.next()) {
        (Some(major), Some(minor)) => format!("{major}.{minor}"),
        _ => version.to_owned(),
    }
}

/// Whether `name` is a valid crate/service name: non-empty, ASCII alphanumerics
/// plus `-`/`_`, beginning with a letter.
pub fn validate_name(name: &str) -> Result<(), ScaffoldError> {
    let starts_with_letter = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
    let body_ok = name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if starts_with_letter && body_ok {
        Ok(())
    } else {
        Err(ScaffoldError::InvalidName(name.to_owned()))
    }
}

/// Keep only the conditional blocks whose condition matches `flags`, dropping all
/// `// __IF …__` / `// __END …__` marker lines. A condition is a flag name, or
/// `!name` for its negation; blocks nest.
fn apply_conditionals(template: &str, flags: &[(&str, bool)]) -> String {
    let is_set = |name: &str| flags.iter().find(|(f, _)| *f == name).is_some_and(|(_, v)| *v);

    let mut out = String::with_capacity(template.len());
    let mut include_stack: Vec<bool> = Vec::new();
    for line in template.lines() {
        let trimmed = line.trim();
        if let Some(cond) = trimmed.strip_prefix("// __IF ").and_then(|s| s.strip_suffix("__")) {
            let included = match cond.strip_prefix('!') {
                Some(name) => !is_set(name),
                None => is_set(cond),
            };
            include_stack.push(included);
        } else if trimmed.strip_prefix("// __END ").and_then(|s| s.strip_suffix("__")).is_some() {
            include_stack.pop();
        } else if include_stack.iter().all(|&inc| inc) {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn render(template: &str, name: &str, options: &Options) -> String {
    let flags = [
        ("jwt", options.with_jwt),
        ("db", options.database.is_some()),
        ("scheduler", options.with_scheduler),
    ];
    let (system, url) = match options.database {
        Some(db) => (db.system().to_owned(), db.sample_url(name)),
        None => (String::new(), String::new()),
    };
    apply_conditionals(template, &flags)
        .replace("__NAME__", name)
        .replace("__KLAUTHED_REQ__", &klauthed_req())
        .replace("__FEATURES__", &feature_list(options))
        .replace("__DB_SYSTEM__", &system)
        .replace("__DB_URL__", &url)
}

/// The files a scaffolded service is made of, as `(relative path, rendered
/// contents)`. Pure (no I/O) so it can be unit-tested.
fn files(name: &str, options: &Options) -> Vec<(PathBuf, String)> {
    vec![
        (PathBuf::from("Cargo.toml"), render(CARGO_TMPL, name, options)),
        (["src", "main.rs"].iter().collect(), render(MAIN_TMPL, name, options)),
        (["config", "default.toml"].iter().collect(), render(CONFIG_TMPL, name, options)),
        (PathBuf::from(".gitignore"), render(GITIGNORE_TMPL, name, options)),
        (PathBuf::from("README.md"), render(README_TMPL, name, options)),
    ]
}

/// Generate a new klauthed service named `name` into `dir`.
///
/// `dir` may not exist yet (it is created) but must be empty if it does. On
/// success the returned paths are the files that were written, relative to `dir`.
///
/// # Errors
/// Returns [`ScaffoldError`] if the name is invalid, the target is non-empty, or
/// a filesystem operation fails.
pub fn scaffold(name: &str, dir: &Path, options: &Options) -> Result<Vec<PathBuf>, ScaffoldError> {
    validate_name(name)?;

    if dir.exists() {
        let mut entries = fs::read_dir(dir)
            .map_err(|source| ScaffoldError::Io { path: dir.to_path_buf(), source })?;
        if entries.next().is_some() {
            return Err(ScaffoldError::TargetExists(dir.to_path_buf()));
        }
    }

    let mut written = Vec::new();
    for (rel, contents) in files(name, options) {
        let target = dir.join(&rel);
        write_file(&target, &contents)?;
        written.push(rel);
    }
    Ok(written)
}

fn write_file(path: &Path, contents: &str) -> Result<(), ScaffoldError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|source| ScaffoldError::Io { path: parent.to_path_buf(), source })?;
    }
    fs::write(path, contents)
        .map_err(|source| ScaffoldError::Io { path: path.to_path_buf(), source })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_and_invalid_names() {
        assert!(validate_name("my-service").is_ok());
        assert!(validate_name("svc_1").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("1svc").is_err());
        assert!(validate_name("bad name").is_err());
        assert!(validate_name("bad/name").is_err());
    }

    #[test]
    fn base_template_renders_name_version_and_no_markers() {
        let opts = Options::default();
        let cargo = render(CARGO_TMPL, "my-service", &opts);
        assert!(cargo.contains("name = \"my-service\""));
        assert!(cargo.contains(&format!("klauthed = {{ version = \"{}\"", klauthed_req())));

        let main = render(MAIN_TMPL, "my-service", &opts);
        assert!(main.contains("hello from my-service"));
        assert!(main.contains("serve_with_defaults"));
        // The base scaffold is minimal: no auth, no database.
        assert!(!main.contains("/login"));
        assert!(!main.contains("serve_with_components"));
    }

    #[test]
    fn with_jwt_adds_security_feature_and_auth_routes() {
        let opts = Options { with_jwt: true, ..Options::default() };
        let cargo = render(CARGO_TMPL, "svc", &opts);
        assert!(cargo.contains("\"security\""));

        let main = render(MAIN_TMPL, "svc", &opts);
        assert!(main.contains("/login") && main.contains("/api"));
        assert!(main.contains("JwtSigner") && main.contains("JwtAuth"));
    }

    #[test]
    fn with_scheduler_starts_interval_and_cron_tasks() {
        let opts = Options { with_scheduler: true, ..Options::default() };
        let cargo = render(CARGO_TMPL, "svc", &opts);
        assert!(cargo.contains("\"scheduler\""));

        let main = render(MAIN_TMPL, "svc", &opts);
        assert!(main.contains("Scheduler::new()"));
        assert!(main.contains(".every("));
        assert!(main.contains(".cron(") && main.contains("Cron::parse("));
        // The base scaffold has no scheduler.
        assert!(!render(MAIN_TMPL, "svc", &Options::default()).contains("Scheduler::new()"));
    }

    #[test]
    fn with_database_wires_a_pool_and_config() {
        let opts = Options { database: Some(Database::Postgres), ..Options::default() };
        let cargo = render(CARGO_TMPL, "svc", &opts);
        assert!(cargo.contains("\"postgres\""));

        let main = render(MAIN_TMPL, "svc", &opts);
        assert!(main.contains("serve_with_components"));
        assert!(main.contains("db::connect"));
        assert!(!main.contains("serve_with_defaults"));

        let config = render(CONFIG_TMPL, "svc", &opts);
        assert!(config.contains("[database]"));
        assert!(config.contains("system = \"postgres\""));
        assert!(config.contains("postgres://localhost:5432/svc"));

        // The base scaffold has no database section.
        assert!(!render(CONFIG_TMPL, "svc", &Options::default()).contains("[database]"));
    }

    #[test]
    fn no_placeholders_or_markers_leak_in_any_combination() {
        for with_jwt in [false, true] {
            for with_scheduler in [false, true] {
                for database in [None, Some(Database::Postgres), Some(Database::Sqlite)] {
                    let opts = Options { with_jwt, database, with_scheduler };
                    for template in
                        [CARGO_TMPL, MAIN_TMPL, CONFIG_TMPL, README_TMPL, GITIGNORE_TMPL]
                    {
                        let out = render(template, "svc", &opts);
                        for leak in ["__IF", "__END", "__NAME__", "__FEATURES__", "__DB_"] {
                            assert!(!out.contains(leak), "{leak} leaked for {opts:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn klauthed_req_is_major_minor() {
        let v = env!("CARGO_PKG_VERSION");
        let expected: String = v.split('.').take(2).collect::<Vec<_>>().join(".");
        assert_eq!(klauthed_req(), expected);
    }
}
