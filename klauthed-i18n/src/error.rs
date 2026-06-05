use klauthed_macros::DomainError;

/// Errors raised while loading message catalogs.
#[derive(Debug, DomainError)]
#[domain(prefix = "i18n", category = "internal")]
pub enum I18nError {
    /// A catalog's TOML could not be parsed.
    Parse { locale: String, message: String },
    /// A catalog file could not be read.
    Io { path: String, message: String },
}

impl std::fmt::Display for I18nError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            I18nError::Parse { locale, message } => {
                write!(f, "failed to parse '{locale}' catalog: {message}")
            }
            I18nError::Io { path, message } => {
                write!(f, "failed to read catalog '{path}': {message}")
            }
        }
    }
}

impl std::error::Error for I18nError {}
