//! The audit event model: [`AuditEvent`], [`AuditOutcome`], and the fluent
//! [`AuditEventBuilder`].

use std::collections::BTreeMap;

use klauthed_core::context::RequestContext;
use klauthed_core::id::Id;
use klauthed_core::time::{Clock, SystemClock, Timestamp};
use serde::{Deserialize, Serialize};

/// Zero-sized marker tagging an [`AuditId`].
pub struct Audit;

/// A typed, time-sortable audit-event identifier.
pub type AuditId = Id<Audit>;

/// The result of the audited action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    /// The action completed successfully.
    Success,
    /// The action was attempted but failed or was denied.
    Failure,
}

impl AuditOutcome {
    /// Whether this is [`Success`](AuditOutcome::Success).
    pub fn is_success(self) -> bool {
        matches!(self, AuditOutcome::Success)
    }
}

/// An immutable record of an audited action.
///
/// Construct via [`AuditEvent::builder`]; fields are read-only afterward.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    id: AuditId,
    occurred_at: Timestamp,
    action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    actor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tenant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resource_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    resource_id: Option<String>,
    outcome: AuditOutcome,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    metadata: BTreeMap<String, String>,
}

impl AuditEvent {
    /// Start building an event for `action` (e.g. `tenant.suspend`).
    pub fn builder(action: impl Into<String>) -> AuditEventBuilder {
        AuditEventBuilder::new(action)
    }

    /// The event id.
    pub fn id(&self) -> AuditId {
        self.id
    }

    /// When the action occurred.
    pub fn occurred_at(&self) -> Timestamp {
        self.occurred_at
    }

    /// The action name.
    pub fn action(&self) -> &str {
        &self.action
    }

    /// The actor / principal who performed the action, if known.
    pub fn actor(&self) -> Option<&str> {
        self.actor.as_deref()
    }

    /// The tenant the action applied to, if any.
    pub fn tenant(&self) -> Option<&str> {
        self.tenant.as_deref()
    }

    /// The affected resource type, if any.
    pub fn resource_type(&self) -> Option<&str> {
        self.resource_type.as_deref()
    }

    /// The affected resource id, if any.
    pub fn resource_id(&self) -> Option<&str> {
        self.resource_id.as_deref()
    }

    /// The outcome.
    pub fn outcome(&self) -> AuditOutcome {
        self.outcome
    }

    /// All metadata.
    pub fn metadata(&self) -> &BTreeMap<String, String> {
        &self.metadata
    }
}

/// Ergonomic builder for [`AuditEvent`].
///
/// Defaults: a fresh [`AuditId`], `occurred_at` from the supplied [`Clock`] (or
/// the system clock via [`build`](AuditEventBuilder::build)), and
/// [`AuditOutcome::Success`].
#[derive(Debug, Clone)]
pub struct AuditEventBuilder {
    id: AuditId,
    occurred_at: Option<Timestamp>,
    action: String,
    actor: Option<String>,
    tenant: Option<String>,
    resource_type: Option<String>,
    resource_id: Option<String>,
    outcome: AuditOutcome,
    metadata: BTreeMap<String, String>,
}

impl AuditEventBuilder {
    fn new(action: impl Into<String>) -> Self {
        Self {
            id: AuditId::new(),
            occurred_at: None,
            action: action.into(),
            actor: None,
            tenant: None,
            resource_type: None,
            resource_id: None,
            outcome: AuditOutcome::Success,
            metadata: BTreeMap::new(),
        }
    }

    /// Override the event id.
    pub fn id(mut self, id: AuditId) -> Self {
        self.id = id;
        self
    }

    /// Set an explicit occurrence time (otherwise taken from the clock at build).
    pub fn occurred_at(mut self, at: Timestamp) -> Self {
        self.occurred_at = Some(at);
        self
    }

    /// Set the actor / principal.
    pub fn actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = Some(actor.into());
        self
    }

    /// Set the tenant.
    pub fn tenant(mut self, tenant: impl Into<String>) -> Self {
        self.tenant = Some(tenant.into());
        self
    }

    /// Set the affected resource (type and id together).
    pub fn resource(mut self, ty: impl Into<String>, id: impl Into<String>) -> Self {
        self.resource_type = Some(ty.into());
        self.resource_id = Some(id.into());
        self
    }

    /// Set the outcome (default [`Success`](AuditOutcome::Success)).
    pub fn outcome(mut self, outcome: AuditOutcome) -> Self {
        self.outcome = outcome;
        self
    }

    /// Mark the outcome as [`Failure`](AuditOutcome::Failure).
    pub fn failed(self) -> Self {
        self.outcome(AuditOutcome::Failure)
    }

    /// Insert a metadata entry.
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Copy `actor`, `tenant`, and the request-id metadata from a
    /// [`RequestContext`] (without overwriting values already set).
    pub fn from_context(mut self, ctx: &RequestContext) -> Self {
        if self.actor.is_none() {
            self.actor = ctx.principal().map(str::to_owned);
        }
        if self.tenant.is_none() {
            self.tenant = ctx.tenant().map(str::to_owned);
        }
        self.metadata
            .entry("request_id".to_owned())
            .or_insert_with(|| ctx.request_id().to_string());
        self
    }

    /// Finish building, stamping `occurred_at` from the system clock if unset.
    pub fn build(self) -> AuditEvent {
        self.build_with(&SystemClock)
    }

    /// Finish building, taking `occurred_at` from `clock` when unset (for tests).
    pub fn build_with(self, clock: &impl Clock) -> AuditEvent {
        AuditEvent {
            id: self.id,
            occurred_at: self.occurred_at.unwrap_or_else(|| clock.now()),
            action: self.action,
            actor: self.actor,
            tenant: self.tenant,
            resource_type: self.resource_type,
            resource_id: self.resource_id,
            outcome: self.outcome,
            metadata: self.metadata,
        }
    }
}
