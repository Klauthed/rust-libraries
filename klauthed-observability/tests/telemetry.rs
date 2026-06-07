//! Public-API integration tests for telemetry configuration.
//!
//! These cover the pure config derivation. `init` installs a process-global
//! tracing subscriber, so it is exercised by the example rather than here.

use klauthed_core::config::Profile;
use klauthed_observability::{LogFormat, TelemetryConfig};

#[test]
fn config_derives_log_format_from_profile() {
    let prod = TelemetryConfig::for_profile(&Profile::Prod, "billing-api");
    assert_eq!(prod.service_name, "billing-api");
    // Vault-required profiles log JSON (machine-ingestible).
    assert_eq!(prod.log.format, LogFormat::Json);

    let local = TelemetryConfig::for_profile(&Profile::Local, "billing-api");
    assert_eq!(local.log.format, LogFormat::Pretty);
}
