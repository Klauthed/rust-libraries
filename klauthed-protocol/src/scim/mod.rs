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
    pub const ENTERPRISE_USER: &str = "urn:ietf:params:scim:schemas:extension:enterprise:2.0:User";
    /// The `ListResponse` message schema URN (RFC 7644).
    pub const LIST_RESPONSE: &str = "urn:ietf:params:scim:api:messages:2.0:ListResponse";
    /// The `PatchOp` message schema URN (RFC 7644).
    pub const PATCH_OP: &str = "urn:ietf:params:scim:api:messages:2.0:PatchOp";
    /// The `Error` message schema URN (RFC 7644).
    pub const ERROR: &str = "urn:ietf:params:scim:api:messages:2.0:Error";
}

pub mod patch;
pub mod resource;

pub use patch::{PatchOp, PatchOpType, PatchOperation};
pub use resource::{Group, ListResponse, Member, Meta, MultiValued, Name, User};
