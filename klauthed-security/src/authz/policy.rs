//! Attribute-based access control (ABAC): policies evaluated over request
//! attributes.
//!
//! Where the RBAC [`Authorizer`](super::Authorizer) answers "does this principal
//! hold permission X?", ABAC answers "is this *request* permitted given its
//! attributes?" — subject, resource, action, and environment values tested by
//! [`Condition`]s. A [`PolicySet`] combines [`Policy`] rules with
//! **deny-overrides** and **default-deny**: a request is permitted only if some
//! `Allow` policy matches and no `Deny` policy matches.
//!
//! ```
//! use klauthed_security::authz::{Attributes, Condition, Decision, Policy, PolicySet};
//!
//! let policies = PolicySet::new()
//!     // Editors and admins may write articles...
//!     .with(Policy::allow(Condition::all([
//!         Condition::eq("action", "articles:write"),
//!         Condition::is_in("subject.role", ["editor", "admin"]),
//!     ])))
//!     // ...but a suspended subject is always denied.
//!     .with(Policy::deny(Condition::eq("subject.status", "suspended")));
//!
//! let active_editor = Attributes::new()
//!     .with("action", "articles:write")
//!     .with("subject.role", "editor")
//!     .with("subject.status", "active");
//! assert_eq!(policies.evaluate(&active_editor), Decision::Permit);
//!
//! // deny-overrides: the Deny rule wins even though the Allow also matches.
//! let suspended = active_editor.clone().with("subject.status", "suspended");
//! assert_eq!(policies.evaluate(&suspended), Decision::Deny);
//! ```

use std::collections::BTreeMap;

use crate::error::SecurityError;

/// A typed attribute value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttrValue {
    /// A text value.
    Str(String),
    /// An integer value.
    Int(i64),
    /// A boolean value.
    Bool(bool),
    /// A list of text values (e.g. a subject's groups).
    List(Vec<String>),
}

impl From<&str> for AttrValue {
    fn from(s: &str) -> Self {
        AttrValue::Str(s.to_owned())
    }
}
impl From<String> for AttrValue {
    fn from(s: String) -> Self {
        AttrValue::Str(s)
    }
}
impl From<i64> for AttrValue {
    fn from(n: i64) -> Self {
        AttrValue::Int(n)
    }
}
impl From<bool> for AttrValue {
    fn from(b: bool) -> Self {
        AttrValue::Bool(b)
    }
}
impl From<Vec<String>> for AttrValue {
    fn from(v: Vec<String>) -> Self {
        AttrValue::List(v)
    }
}

/// The attributes describing an access request: a flat map of dotted keys (by
/// convention `subject.*`, `resource.*`, `action`, `env.*`) to [`AttrValue`]s.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attributes {
    map: BTreeMap<String, AttrValue>,
}

impl Attributes {
    /// An empty attribute set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert an attribute, builder-style.
    #[must_use]
    pub fn with(mut self, key: impl Into<String>, value: impl Into<AttrValue>) -> Self {
        self.map.insert(key.into(), value.into());
        self
    }

    /// Insert an attribute in place.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<AttrValue>) {
        self.map.insert(key.into(), value.into());
    }

    /// Look up an attribute value.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&AttrValue> {
        self.map.get(key)
    }
}

/// A predicate over an [`Attributes`] set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Condition {
    /// Always matches (an unconditional rule).
    Always,
    /// The attribute at `key` equals the given value.
    Eq(String, AttrValue),
    /// The attribute at `key` is a [`Str`](AttrValue::Str) equal to one of the
    /// listed values.
    In(String, Vec<String>),
    /// The attribute at `key` is a [`List`](AttrValue::List) containing the value.
    Contains(String, String),
    /// An attribute is present at `key`.
    Present(String),
    /// All sub-conditions match (logical AND; vacuously true when empty).
    All(Vec<Condition>),
    /// Any sub-condition matches (logical OR; vacuously false when empty).
    Any(Vec<Condition>),
    /// The sub-condition does not match.
    Not(Box<Condition>),
}

impl Condition {
    /// `key == value`.
    pub fn eq(key: impl Into<String>, value: impl Into<AttrValue>) -> Self {
        Condition::Eq(key.into(), value.into())
    }

    /// `key` is a string in `values`.
    pub fn is_in(
        key: impl Into<String>,
        values: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Condition::In(key.into(), values.into_iter().map(Into::into).collect())
    }

    /// `key` is a list containing `item`.
    pub fn contains(key: impl Into<String>, item: impl Into<String>) -> Self {
        Condition::Contains(key.into(), item.into())
    }

    /// `key` is present.
    pub fn present(key: impl Into<String>) -> Self {
        Condition::Present(key.into())
    }

    /// Logical AND of `conditions`.
    pub fn all(conditions: impl IntoIterator<Item = Condition>) -> Self {
        Condition::All(conditions.into_iter().collect())
    }

    /// Logical OR of `conditions`.
    pub fn any(conditions: impl IntoIterator<Item = Condition>) -> Self {
        Condition::Any(conditions.into_iter().collect())
    }

    /// Negation of `condition`.
    pub fn negate(condition: Condition) -> Self {
        Condition::Not(Box::new(condition))
    }

    /// Evaluate the condition against `attrs`.
    #[must_use]
    pub fn evaluate(&self, attrs: &Attributes) -> bool {
        match self {
            Condition::Always => true,
            Condition::Eq(key, value) => attrs.get(key) == Some(value),
            Condition::In(key, values) => {
                matches!(attrs.get(key), Some(AttrValue::Str(s)) if values.contains(s))
            }
            Condition::Contains(key, item) => {
                matches!(attrs.get(key), Some(AttrValue::List(list)) if list.contains(item))
            }
            Condition::Present(key) => attrs.get(key).is_some(),
            Condition::All(conditions) => conditions.iter().all(|c| c.evaluate(attrs)),
            Condition::Any(conditions) => conditions.iter().any(|c| c.evaluate(attrs)),
            Condition::Not(condition) => !condition.evaluate(attrs),
        }
    }
}

/// Whether a matching [`Policy`] grants or refuses access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Permit the request when the policy's condition matches.
    Allow,
    /// Refuse the request when the policy's condition matches (wins ties).
    Deny,
}

/// A single rule: an [`Effect`] applied when a [`Condition`] matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    /// The effect applied when [`condition`](Policy::condition) matches.
    pub effect: Effect,
    /// The predicate that activates this policy.
    pub condition: Condition,
}

impl Policy {
    /// A policy that permits when `condition` matches.
    #[must_use]
    pub fn allow(condition: Condition) -> Self {
        Self { effect: Effect::Allow, condition }
    }

    /// A policy that denies when `condition` matches.
    #[must_use]
    pub fn deny(condition: Condition) -> Self {
        Self { effect: Effect::Deny, condition }
    }
}

/// The outcome of evaluating a [`PolicySet`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Access is granted.
    Permit,
    /// Access is refused.
    Deny,
}

impl Decision {
    /// Whether access was granted.
    #[must_use]
    pub fn is_permit(&self) -> bool {
        matches!(self, Decision::Permit)
    }
}

/// An ordered collection of [`Policy`] rules evaluated with **deny-overrides**
/// and **default-deny**.
#[derive(Debug, Clone, Default)]
pub struct PolicySet {
    policies: Vec<Policy>,
}

impl PolicySet {
    /// An empty policy set (which permits nothing — default deny).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a policy, builder-style.
    #[must_use]
    pub fn with(mut self, policy: Policy) -> Self {
        self.policies.push(policy);
        self
    }

    /// Add a policy in place.
    pub fn push(&mut self, policy: Policy) {
        self.policies.push(policy);
    }

    /// Evaluate `attrs` against every policy.
    ///
    /// A matching `Deny` immediately wins (deny-overrides). Otherwise the request
    /// is permitted only if at least one `Allow` matched; with no matching policy
    /// the result is [`Decision::Deny`] (default-deny).
    #[must_use]
    pub fn evaluate(&self, attrs: &Attributes) -> Decision {
        let mut permitted = false;
        for policy in &self.policies {
            if policy.condition.evaluate(attrs) {
                match policy.effect {
                    Effect::Deny => return Decision::Deny,
                    Effect::Allow => permitted = true,
                }
            }
        }
        if permitted { Decision::Permit } else { Decision::Deny }
    }

    /// Evaluate and convert a [`Decision::Deny`] into
    /// [`SecurityError::Forbidden`], for call sites that propagate `Result`.
    ///
    /// # Errors
    /// [`SecurityError::Forbidden`] when the decision is [`Decision::Deny`].
    pub fn authorize(&self, attrs: &Attributes) -> Result<(), SecurityError> {
        match self.evaluate(attrs) {
            Decision::Permit => Ok(()),
            Decision::Deny => Err(SecurityError::Forbidden),
        }
    }
}
