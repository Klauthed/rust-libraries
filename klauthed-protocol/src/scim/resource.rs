//! SCIM 2.0 core resource types: [`User`], [`Group`], and their building blocks
//! ([`Meta`], [`Name`], [`MultiValued`], [`Member`], [`ListResponse`]).

use serde::{Deserialize, Serialize};

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
/// [`super::schema::USER`] and any extension URNs.
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
    /// [`super::schema::LIST_RESPONSE`] URN.
    pub fn new(resources: Vec<T>) -> Self {
        let total = resources.len();
        ListResponse {
            schemas: vec![super::schema::LIST_RESPONSE.to_owned()],
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
            schemas: vec![super::schema::LIST_RESPONSE.to_owned()],
            total_results: 0,
            items_per_page: None,
            start_index: None,
            resources: Vec::new(),
        }
    }
}
