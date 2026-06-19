//! Service scaffolding: render the embedded templates into a new project tree.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

// Templates embedded at compile time; rendered by substituting `__NAME__` and
// `__KLAUTHED_REQ__`.
const CARGO_TMPL: &str = include_str!("../templates/Cargo.toml.tmpl");
const MAIN_TMPL: &str = include_str!("../templates/main.rs.tmpl");
const MAIN_JWT_TMPL: &str = include_str!("../templates/main.jwt.rs.tmpl");
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

/// Options controlling what a scaffolded service includes.
#[derive(Debug, Default, Clone, Copy)]
pub struct Options {
    /// Include JWT authentication: the `security` feature plus `/login` and a
    /// JWT-protected `/api/me` route.
    pub with_jwt: bool,
}

/// The `klauthed` features the generated project enables, as a TOML array body
/// (e.g. `"core", "web", "observability"`).
fn feature_list(options: &Options) -> String {
    let mut features = vec!["core", "web", "observability"];
    if options.with_jwt {
        features.push("security");
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

fn render(template: &str, name: &str, options: &Options) -> String {
    template
        .replace("__NAME__", name)
        .replace("__KLAUTHED_REQ__", &klauthed_req())
        .replace("__FEATURES__", &feature_list(options))
}

/// The files a scaffolded service is made of, as `(relative path, rendered
/// contents)`. Pure (no I/O) so it can be unit-tested.
fn files(name: &str, options: &Options) -> Vec<(PathBuf, String)> {
    let main_template = if options.with_jwt { MAIN_JWT_TMPL } else { MAIN_TMPL };
    vec![
        (PathBuf::from("Cargo.toml"), render(CARGO_TMPL, name, options)),
        (["src", "main.rs"].iter().collect(), render(main_template, name, options)),
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
    fn templates_render_the_name_and_version() {
        let opts = Options::default();
        let cargo = render(CARGO_TMPL, "my-service", &opts);
        assert!(cargo.contains("name = \"my-service\""));
        assert!(cargo.contains(&format!("klauthed = {{ version = \"{}\"", klauthed_req())));
        assert!(!cargo.contains("__NAME__"));
        assert!(!cargo.contains("__KLAUTHED_REQ__"));
        assert!(!cargo.contains("__FEATURES__"));

        let main = render(MAIN_TMPL, "my-service", &opts);
        assert!(main.contains("hello from my-service"));
        assert!(!main.contains("__NAME__"));
    }

    #[test]
    fn with_jwt_adds_the_security_feature_and_auth_routes() {
        let opts = Options { with_jwt: true };
        let cargo = render(CARGO_TMPL, "svc", &opts);
        assert!(cargo.contains("\"security\""), "jwt scaffold enables the security feature");

        let main = render(MAIN_JWT_TMPL, "svc", &opts);
        assert!(main.contains("/login") && main.contains("/api"));
        assert!(main.contains("JwtSigner") && main.contains("JwtAuth"));

        // The default (no-jwt) scaffold stays minimal.
        let base = render(CARGO_TMPL, "svc", &Options::default());
        assert!(!base.contains("\"security\""));
    }

    #[test]
    fn klauthed_req_is_major_minor() {
        // Whatever this crate's version is, the requirement is its major.minor.
        let v = env!("CARGO_PKG_VERSION");
        let expected: String = v.split('.').take(2).collect::<Vec<_>>().join(".");
        assert_eq!(klauthed_req(), expected);
    }
}
