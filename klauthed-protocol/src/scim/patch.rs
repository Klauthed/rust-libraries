//! SCIM 2.0 PATCH types ([`PatchOp`], [`PatchOperation`], [`PatchOpType`],
//! RFC 7644 §3.5.2).

use serde::{Deserialize, Serialize};

/// A SCIM PATCH operation kind (RFC 7644 section 3.5.2).
///
/// Deserialization is case-insensitive per the spec ("Operation values MUST be
/// one of 'add', 'remove', or 'replace'" but are matched case-insensitively);
/// serialization always emits lowercase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum PatchOpType {
    /// Add a new attribute value.
    Add,
    /// Remove an attribute value.
    Remove,
    /// Replace an existing attribute value.
    Replace,
}

impl<'de> Deserialize<'de> for PatchOpType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        // RFC 7644: operation values are matched case-insensitively.
        match raw.to_ascii_lowercase().as_str() {
            "add" => Ok(PatchOpType::Add),
            "remove" => Ok(PatchOpType::Remove),
            "replace" => Ok(PatchOpType::Replace),
            other => Err(serde::de::Error::custom(format!(
                "unknown SCIM patch op '{other}' (expected add, remove, or replace)"
            ))),
        }
    }
}

/// A single operation within a [`PatchOp`] (RFC 7644 section 3.5.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatchOperation {
    /// The operation kind.
    pub op: PatchOpType,

    /// An attribute path (SCIM path filter). Optional for `add`/`replace`
    /// targeting the resource root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// The value to add or replace. Omitted for most `remove` operations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
}

/// A SCIM PATCH request body (RFC 7644 section 3.5.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatchOp {
    /// The schema URNs of the request message.
    pub schemas: Vec<String>,

    /// The ordered operations to apply.
    #[serde(rename = "Operations")]
    pub operations: Vec<PatchOperation>,
}

impl PatchOp {
    /// Build a PATCH body carrying the [`super::schema::PATCH_OP`] URN.
    pub fn new(operations: Vec<PatchOperation>) -> Self {
        PatchOp { schemas: vec![super::schema::PATCH_OP.to_owned()], operations }
    }
}
