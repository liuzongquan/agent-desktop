use super::*;

#[test]
fn android_source_preserves_hierarchy_identity_and_capabilities() {
    let tree = parse_source(
        r#"<hierarchy><android.widget.Button text="Save &amp; close"
        resource-id="app:id/save" content-desc="Save" clickable="true" enabled="true"
        bounds="[10,20][110,60]"/><android.widget.EditText text="draft" content-desc="Message"
        resource-id="app:id/message" bounds="[10,70][210,120]" focused="true"/></hierarchy>"#,
        MobilePlatform::Android,
    )
    .unwrap();
    assert_eq!(tree.role, "window");
    assert_eq!(tree.children.len(), 2);
    let button = &tree.children[0];
    assert_eq!(button.role, "button");
    assert_eq!(button.identity.name.as_deref(), Some("Save"));
    assert_eq!(button.identity.value.as_deref(), Some("Save & close"));
    assert_eq!(
        button.identity.native_id.as_ref().unwrap().value,
        "app:id/save"
    );
    assert!(button.identity.retained_object.is_none());
    assert_eq!(
        button.presentation.bounds.unwrap(),
        Rect {
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 40.0
        }
    );
    assert!(
        button
            .presentation
            .available_actions
            .iter()
            .any(|action| action == "Click")
    );
    let field = &tree.children[1];
    assert_eq!(field.role, "textfield");
    assert_eq!(field.identity.name.as_deref(), Some("Message"));
    assert_eq!(field.identity.value.as_deref(), Some("draft"));
    assert!(
        field
            .presentation
            .states
            .iter()
            .any(|state| state == "focused")
    );
    assert!(
        field
            .presentation
            .available_actions
            .iter()
            .any(|action| action == "SetValue")
    );
}

#[test]
fn android_editable_text_is_never_a_stable_name() {
    let tree = parse_source(
        r#"<node class="android.widget.EditText" text="changing value"/>"#,
        MobilePlatform::Android,
    )
    .unwrap();
    assert!(tree.identity.name.is_none());
    assert_eq!(tree.identity.value.as_deref(), Some("changing value"));
}

#[test]
fn ios_source_maps_switch_text_and_geometry() {
    let tree = parse_source(
        r#"<AppiumAUT><XCUIElementTypeApplication name="Demo">
      <XCUIElementTypeSwitch name="wifi" label="Wi-Fi" value="1" enabled="true"
        visible="true" x="2" y="3" width="50" height="30"/>
      <XCUIElementTypeTextView name="editor" label="Message" value="Hello"/>
      <XCUIElementTypeStaticText label="Read only"/>
    </XCUIElementTypeApplication></AppiumAUT>"#,
        MobilePlatform::Ios,
    )
    .unwrap();
    let children = &tree.children[0].children;
    assert_eq!(children[0].role, "switch");
    assert!(
        children[0]
            .presentation
            .states
            .iter()
            .any(|state| state == "checked")
    );
    assert_eq!(
        children[0].identity.native_id.as_ref().unwrap().value,
        "wifi"
    );
    assert_eq!(children[1].role, "textfield");
    assert_eq!(children[2].role, "statictext");
}

#[test]
fn secure_values_are_redacted_on_both_platforms() {
    for (platform, xml) in [
        (
            MobilePlatform::Android,
            r#"<node class="android.widget.EditText" text="private-value" password="true"/>"#,
        ),
        (
            MobilePlatform::Ios,
            r#"<XCUIElementTypeSecureTextField label="Password" value="private-value"/>"#,
        ),
    ] {
        let tree = parse_source(xml, platform).unwrap();
        assert!(tree.identity.value.is_none());
        assert!(
            tree.presentation
                .states
                .iter()
                .any(|state| state == "secure")
        );
        assert!(
            !serde_json::to_string(&tree)
                .unwrap()
                .contains("private-value")
        );
    }
}

#[test]
fn malformed_and_entity_expanding_sources_fail_closed() {
    for xml in [
        "<broken>",
        "<!DOCTYPE root [<!ENTITY x 'secret'>]><root>&x;</root>",
    ] {
        assert_eq!(
            parse_source(xml, MobilePlatform::Android).unwrap_err().code,
            ErrorCode::ActionFailed
        );
    }
}

#[test]
fn invalid_and_nonfinite_geometry_is_rejected() {
    for (platform, xml) in [
        (MobilePlatform::Android, r#"<node bounds="[20,0][10,30]"/>"#),
        (MobilePlatform::Android, r#"<node bounds="invalid"/>"#),
        (
            MobilePlatform::Ios,
            r#"<node x="NaN" y="0" width="10" height="10"/>"#,
        ),
        (MobilePlatform::Ios, r#"<node x="0"/>"#),
    ] {
        assert_eq!(
            parse_source(xml, platform).unwrap_err().code,
            ErrorCode::ActionFailed
        );
    }
}

#[test]
fn source_size_node_and_depth_budgets_fail_instead_of_claiming_completeness() {
    let large = " ".repeat(MAX_SOURCE_BYTES + 1);
    assert_eq!(
        parse_source(&large, MobilePlatform::Android)
            .unwrap_err()
            .code,
        ErrorCode::Timeout
    );
    let many = format!("<root>{}</root>", "<node/>".repeat(MAX_NODES as usize));
    assert_eq!(
        parse_source(&many, MobilePlatform::Android)
            .unwrap_err()
            .code,
        ErrorCode::Timeout
    );
    let deep = format!(
        "{}{}",
        "<node>".repeat(MAX_DEPTH + 2),
        "</node>".repeat(MAX_DEPTH + 2)
    );
    assert_eq!(
        parse_source(&deep, MobilePlatform::Android)
            .unwrap_err()
            .code,
        ErrorCode::Timeout
    );
}
