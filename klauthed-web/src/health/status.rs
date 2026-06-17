//! `HealthStatus` — the three-valued readiness signal.

use serde::Serialize;

/// Health of a single component or of the service overall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    /// Fully healthy.
    Up,
    /// Reachable but impaired (e.g. degraded latency, partial capacity).
    Degraded,
    /// Unhealthy / unreachable.
    Down,
}

impl HealthStatus {
    /// Whether this status counts as ready (only [`HealthStatus::Up`] does).
    #[must_use]
    pub fn is_ready(self) -> bool {
        matches!(self, HealthStatus::Up)
    }

    /// The lowercase wire string (`"up"`, `"degraded"`, `"down"`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            HealthStatus::Up => "up",
            HealthStatus::Degraded => "degraded",
            HealthStatus::Down => "down",
        }
    }

    /// The worse (less healthy) of two statuses, ordered `Up < Degraded < Down`.
    #[must_use]
    pub(super) fn worse(self, other: HealthStatus) -> HealthStatus {
        self.max(other)
    }
}

impl PartialOrd for HealthStatus {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HealthStatus {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn rank(s: HealthStatus) -> u8 {
            match s {
                HealthStatus::Up => 0,
                HealthStatus::Degraded => 1,
                HealthStatus::Down => 2,
            }
        }
        rank(*self).cmp(&rank(*other))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordering_and_readiness() {
        assert!(HealthStatus::Up.is_ready());
        assert!(!HealthStatus::Degraded.is_ready());
        assert!(!HealthStatus::Down.is_ready());
        assert!(HealthStatus::Up < HealthStatus::Degraded);
        assert!(HealthStatus::Degraded < HealthStatus::Down);
        assert_eq!(HealthStatus::Up.worse(HealthStatus::Down), HealthStatus::Down);
    }

    #[test]
    fn as_str_returns_lowercase() {
        assert_eq!(HealthStatus::Up.as_str(), "up");
        assert_eq!(HealthStatus::Degraded.as_str(), "degraded");
        assert_eq!(HealthStatus::Down.as_str(), "down");
    }
}
