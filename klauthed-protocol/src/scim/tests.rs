//! Tests for the SCIM resource and PATCH types.

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
