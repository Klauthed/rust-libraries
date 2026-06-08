use tracing_subscriber::{EnvFilter, Layer, Registry, fmt};

use crate::config::{LogConfig, LogFormat};

/// A type-erased layer over the [`Registry`], so differently-formatted layers
/// can be collected into one `Vec` and composed uniformly.
pub(crate) type BoxedLayer = Box<dyn Layer<Registry> + Send + Sync + 'static>;

/// Build the level filter: `RUST_LOG` if set, else the configured directive,
/// else `info`.
pub(crate) fn env_filter(config: &LogConfig) -> EnvFilter {
    EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(&config.level))
        .unwrap_or_else(|_| EnvFilter::new("info"))
}

/// Build the formatting layer for the configured output format.
pub(crate) fn fmt_layer(config: &LogConfig) -> BoxedLayer {
    match config.format {
        LogFormat::Pretty => fmt::layer().pretty().with_ansi(config.ansi).boxed(),
        LogFormat::Compact => fmt::layer().compact().with_ansi(config.ansi).boxed(),
        LogFormat::Json => fmt::layer().json().with_current_span(true).flatten_event(true).boxed(),
    }
}
