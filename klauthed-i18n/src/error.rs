use klauthed_macros::DomainError;

/// Errors raised while loading message catalogs.
#[derive(Debug, DomainError)]
#[domain(prefix = "i18n", category = "internal")]
pub enum I18nError {
    /// A catalog's TOML could not be parsed.
    Parse {
        /// The locale whose catalog failed to parse.
        locale: String,
        /// The underlying parser message.
        message: String,
    },
    /// A catalog file could not be read.
    Io {
        /// Path of the catalog file that could not be read.
        path: String,
        /// The underlying I/O error message.
        message: String,
    },
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
