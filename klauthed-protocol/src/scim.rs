//! SCIM 2.0 core resource types (RFC 7643 / RFC 7644).
//!
//! Spec-accurate serde models for the SCIM `User` and `Group` resources, the
//! list-response envelope, and the PATCH operation body. These are pure data
//! types: no provisioning logic, no HTTP.
//!
//! SCIM mixes camelCase attribute names (`userName`, `externalId`,
//! `displayName`), capitalized envelope keys (`Resources`, `Operations`), and
//! a `$ref` key, so every non-snake_case field carries an explicit
//! `#[serde(rename = "...")]`. Canonical schema URNs are provided as constants
//! in [`schema`].
//!
//! ```
//! use klauthed_protocol::scim::{User, schema};
//!
//! let user = User {
//!     schemas: vec![schema::USER.into()],
//!     user_name: Some("bjensen@example.com".into()),
//!     ..Default::default()
//! };
//! let json = serde_json::to_value(&user).unwrap();
//! assert_eq!(json["userName"], "bjensen@example.com");
//! assert_eq!(json["schemas"][0], schema::USER);
//! ```

use serde::{Deserialize, Serialize};

/// Canonical SCIM 2.0 schema URNs (RFC 7643).
pub mod schema {
    /// The core `User` resource schema URN.
    pub const USER: &str = "urn:ietf:params:scim:schemas:core:2.0:User";
    /// The core `Group` resource schema URN.
    pub const GROUP: &str = "urn:ietf:params:scim:schemas:core:2.0:Group";
    /// The `ServiceProviderConfig` schema URN.
    pub const SERVICE_PROVIDER_CONFIG: &str =
        "urn:ietf:params:scim:schemas:core:2.0:ServiceProviderConfig";
    /// The Enterprise User extension schema URN.
    pub const ENTERPRISE_USER: &str =
        "urn:ietf:params:scim:schemas:extension:enterprise:2.0:User";
    /// The `ListResponse` message schema URN (RFC 7644).
    pub const LIST_RESPONSE: &str = "urn:ietf:params:scim:api:messages:2.0:ListResponse";
    /// The `PatchOp` message schema URN (RFC 7644).
    pub const PATCH_OP: &str = "urn:ietf:params:scim:api:messages:2.0:PatchOp";
    /// The `Error` message schema URN (RFC 7644).
    pub const ERROR: &str = "urn:ietf:params:scim:api:messages:2.0:Error";
}

/// Common metadata attached to every SCIM resource (RFC 7643 section 3.1).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    /// The resource type (e.g. `"User"`, `"Group"`).
    #[serde(rename = "resourceType", default, skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<String>,

    /// The `DateTime` the resource was created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,

    /// The most recent `DateTime` the resource was modified.
    #[serde(rename = "lastModified", default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,

    /// The resource's canonical URI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,

    /// The resource's version (entity tag).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// The structured `name` attribute of a SCIM `User` (RFC 7643 section 4.1.1).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Name {
    /// The full name, formatted for display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formatted: Option<String>,

    /// The family name (surname).
    #[serde(rename = "familyName", default, skip_serializing_if = "Option::is_none")]
    pub family_name: Option<String>,

    /// The given name (first name).
    #[serde(rename = "givenName", default, skip_serializing_if = "Option::is_none")]
    pub given_name: Option<String>,

    /// The middle name.
    #[serde(rename = "middleName", default, skip_serializing_if = "Option::is_none")]
    pub middle_name: Option<String>,

    /// The honorific prefix (e.g. `"Ms."`).
    #[serde(rename = "honorificPrefix", default, skip_serializing_if = "Option::is_none")]
    pub honorific_prefix: Option<String>,

    /// The honorific suffix (e.g. `"III"`).
    #[serde(rename = "honorificSuffix", default, skip_serializing_if = "Option::is_none")]
    pub honorific_suffix: Option<String>,
}

/// A multi-valued attribute entry (email, phone number, IM, …) per RFC 7643
/// section 2.4.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiValued {
    /// The attribute's significant value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,

    /// A human-readable display label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,

    /// A label indicating the attribute's function (e.g. `"work"`, `"home"`).
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,

    /// Whether this is the primary entry among the collection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<bool>,
}

/// A SCIM `User` resource (RFC 7643 section 4.1).
///
/// Almost every attribute is optional. `schemas` should contain
/// [`schema::USER`] and any extension URNs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    /// The schema URNs this resource conforms to.
    pub schemas: Vec<String>,

    /// The service-provider-assigned unique identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// The client-provided external identifier.
    #[serde(rename = "externalId", default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,

    /// The unique, login identifier for the user.
    #[serde(rename = "userName", default, skip_serializing_if = "Option::is_none")]
    pub user_name: Option<String>,

    /// The components of the user's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<Name>,

    /// The name displayed to end users.
    #[serde(rename = "displayName", default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,

    /// The casual way to address the user.
    #[serde(rename = "nickName", default, skip_serializing_if = "Option::is_none")]
    pub nick_name: Option<String>,

    /// A URI of the user's online profile.
    #[serde(rename = "profileUrl", default, skip_serializing_if = "Option::is_none")]
    pub profile_url: Option<String>,

    /// The user's job title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    /// The kind of user account (e.g. `"Employee"`).
    #[serde(rename = "userType", default, skip_serializing_if = "Option::is_none")]
    pub user_type: Option<String>,

    /// The user's preferred written/spoken language (BCP47 tag).
    #[serde(rename = "preferredLanguage", default, skip_serializing_if = "Option::is_none")]
    pub preferred_language: Option<String>,

    /// The user's default locale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,

    /// The user's time zone (IANA name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,

    /// Whether the user account is active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,

    /// Email addresses for the user.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emails: Vec<MultiValued>,

    /// Phone numbers for the user.
    #[serde(rename = "phoneNumbers", default, skip_serializing_if = "Vec::is_empty")]
    pub phone_numbers: Vec<MultiValued>,

    /// Physical mailing addresses (free-form here; a full address is a
    /// complex multi-valued attribute in the spec).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub addresses: Vec<MultiValued>,

    /// The groups the user belongs to (read-only on the server side).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<MultiValued>,

    /// Resource metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

/// A member of a SCIM `Group` (RFC 7643 section 4.2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    /// The identifier of the member resource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,

    /// A human-readable name for the member.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,

    /// The URI of the member resource.
    #[serde(rename = "$ref", default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,

    /// The type of member (e.g. `"User"`, `"Group"`).
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

/// A SCIM `Group` resource (RFC 7643 section 4.2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    /// The schema URNs this resource conforms to.
    pub schemas: Vec<String>,

    /// The service-provider-assigned unique identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    /// The client-provided external identifier.
    #[serde(rename = "externalId", default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,

    /// The name displayed for the group.
    #[serde(rename = "displayName", default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,

    /// The members of the group.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<Member>,

    /// Resource metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

/// The SCIM list-response envelope (RFC 7644 section 3.4.2).
///
/// Generic over the resource type so the same envelope serves
/// `ListResponse<User>`, `ListResponse<Group>`, etc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListResponse<T> {
    /// The schema URNs of the response message.
    pub schemas: Vec<String>,

    /// The total number of results matching the query.
    #[serde(rename = "totalResults")]
    pub total_results: usize,

    /// The number of results returned in this page.
    #[serde(rename = "itemsPerPage", default, skip_serializing_if = "Option::is_none")]
    pub items_per_page: Option<usize>,

    /// The 1-based index of the first result in this page.
    #[serde(rename = "startIndex", default, skip_serializing_if = "Option::is_none")]
    pub start_index: Option<usize>,

    /// The page of resources.
    #[serde(rename = "Resources", default = "Vec::new")]
    pub resources: Vec<T>,
}

impl<T> ListResponse<T> {
    /// Build a single-page list response carrying the
    /// [`schema::LIST_RESPONSE`] URN.
    pub fn new(resources: Vec<T>) -> Self {
        let total = resources.len();
        ListResponse {
            schemas: vec![schema::LIST_RESPONSE.to_owned()],
            total_results: total,
            items_per_page: Some(total),
            start_index: Some(1),
            resources,
        }
    }
}

impl<T> Default for ListResponse<T> {
    fn default() -> Self {
        ListResponse {
            schemas: vec![schema::LIST_RESPONSE.to_owned()],
            total_results: 0,
            items_per_page: None,
            start_index: None,
            resources: Vec::new(),
        }
    }
}

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
    /// Build a PATCH body carrying the [`schema::PATCH_OP`] URN.
    pub fn new(operations: Vec<PatchOperation>) -> Self {
        PatchOp {
            schemas: vec![schema::PATCH_OP.to_owned()],
            operations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_uses_exact_scim_keys() {
        let user = User {
            schemas: vec![schema::USER.into()],
            id: Some("2819c223-7f76-453a-919d-413861904646".into()),
            external_id: Some("ext-1".into()),
            user_name: Some("bjensen@example.com".into()),
            name: Some(Name {
                formatted: Some("Ms. Barbara J Jensen III".into()),
                family_name: Some("Jensen".into()),
                given_name: Some("Barbara".into()),
                ..Default::default()
            }),
            display_name: Some("Babs Jensen".into()),
            active: Some(true),
            emails: vec![MultiValued {
                value: Some("bjensen@example.com".into()),
                kind: Some("work".into()),
                primary: Some(true),
                ..Default::default()
            }],
            meta: Some(Meta {
                resource_type: Some("User".into()),
                version: Some("W/\"abc\"".into()),
                ..Default::default()
            }),
            ..Default::default()
        };

        let json = serde_json::to_value(&user).unwrap();
        assert_eq!(json["userName"], "bjensen@example.com");
        assert_eq!(json["externalId"], "ext-1");
        assert_eq!(json["displayName"], "Babs Jensen");
        assert_eq!(json["name"]["familyName"], "Jensen");
        assert_eq!(json["name"]["givenName"], "Barbara");
        assert_eq!(json["emails"][0]["type"], "work");
        assert_eq!(json["emails"][0]["primary"], true);
        assert_eq!(json["meta"]["resourceType"], "User");
        assert_eq!(json["schemas"][0], schema::USER);
        // Unset optionals are omitted.
        assert!(json.get("nickName").is_none());
        assert!(json["name"].get("middleName").is_none());
    }

    #[test]
    fn user_round_trips() {
        let json = r#"{
            "schemas": ["urn:ietf:params:scim:schemas:core:2.0:User"],
            "id": "abc",
            "userName": "bjensen@example.com",
            "name": {"givenName": "Barbara", "familyName": "Jensen"},
            "emails": [{"value": "bjensen@example.com", "type": "work", "primary": true}],
            "active": true,
            "meta": {"resourceType": "User", "created": "2011-08-01T18:29:49.793Z"}
        }"#;
        let user: User = serde_json::from_str(json).unwrap();
        assert_eq!(user.user_name.as_deref(), Some("bjensen@example.com"));
        assert_eq!(user.name.as_ref().unwrap().given_name.as_deref(), Some("Barbara"));
        assert_eq!(user.emails[0].kind.as_deref(), Some("work"));
        assert_eq!(user.meta.as_ref().unwrap().resource_type.as_deref(), Some("User"));
    }

    #[test]
    fn group_member_uses_ref_key() {
        let group = Group {
            schemas: vec![schema::GROUP.into()],
            id: Some("g-1".into()),
            display_name: Some("Tour Guides".into()),
            members: vec![Member {
                value: Some("user-1".into()),
                display: Some("Babs Jensen".into()),
                reference: Some("https://example.com/v2/Users/user-1".into()),
                kind: Some("User".into()),
            }],
            ..Default::default()
        };
        let json = serde_json::to_value(&group).unwrap();
        assert_eq!(json["displayName"], "Tour Guides");
        assert_eq!(json["members"][0]["$ref"], "https://example.com/v2/Users/user-1");
        assert_eq!(json["members"][0]["value"], "user-1");
        assert_eq!(json["members"][0]["type"], "User");
        assert_eq!(json["schemas"][0], schema::GROUP);
    }

    #[test]
    fn list_response_uses_exact_envelope_keys() {
        let list = ListResponse::new(vec![User {
            schemas: vec![schema::USER.into()],
            user_name: Some("a@example.com".into()),
            ..Default::default()
        }]);
        let json = serde_json::to_value(&list).unwrap();
        assert_eq!(json["totalResults"], 1);
        assert_eq!(json["itemsPerPage"], 1);
        assert_eq!(json["startIndex"], 1);
        assert!(json.get("Resources").is_some());
        assert_eq!(json["Resources"][0]["userName"], "a@example.com");
        assert_eq!(json["schemas"][0], schema::LIST_RESPONSE);
    }

    #[test]
    fn patch_op_keys_and_case_insensitive_op() {
        let json = r#"{
            "schemas": ["urn:ietf:params:scim:api:messages:2.0:PatchOp"],
            "Operations": [
                {"op": "Replace", "path": "active", "value": false},
                {"op": "ADD", "path": "emails", "value": [{"value": "x@example.com"}]},
                {"op": "remove", "path": "name.givenName"}
            ]
        }"#;
        let patch: PatchOp = serde_json::from_str(json).unwrap();
        assert_eq!(patch.operations.len(), 3);
        // Case-insensitive per RFC 7644.
        assert_eq!(patch.operations[0].op, PatchOpType::Replace);
        assert_eq!(patch.operations[1].op, PatchOpType::Add);
        assert_eq!(patch.operations[2].op, PatchOpType::Remove);

        // Serialization emits canonical lowercase under the "Operations" key.
        let reser = serde_json::to_value(&patch).unwrap();
        assert!(reser.get("Operations").is_some());
        assert_eq!(reser["Operations"][0]["op"], "replace");
        assert_eq!(reser["Operations"][1]["op"], "add");
        assert_eq!(reser["Operations"][2]["op"], "remove");
        // remove op without a value omits the value key.
        assert!(reser["Operations"][2].get("value").is_none());
    }

    #[test]
    fn patch_op_builder_sets_schema() {
        let patch = PatchOp::new(vec![PatchOperation {
            op: PatchOpType::Replace,
            path: Some("active".into()),
            value: Some(serde_json::json!(false)),
        }]);
        assert_eq!(patch.schemas, vec![schema::PATCH_OP.to_owned()]);
    }
}
