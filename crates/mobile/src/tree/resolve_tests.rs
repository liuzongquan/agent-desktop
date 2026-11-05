use super::*;
use agent_desktop_core::{
    ErrorCode, RefCapabilities, RefEntryIdentity, RefGeometry, RefProcess, RefScope, RefSource,
};

#[test]
fn xpath_literals_cannot_inject_a_predicate() {
    assert_eq!(xpath_literal("simple"), "'simple'");
    assert_eq!(xpath_literal("it's"), "\"it's\"");
    assert_eq!(xpath_literal("a'\"b"), "concat('a',\"'\",'\"b')");
}

#[test]
fn parser_identity_and_xpath_share_native_fields() {
    let node = super::super::source::parse_source(r#"<node class="android.widget.Button" resource-id="pkg:id/save" content-desc="Save" text="Save" enabled="true"/>"#, MobilePlatform::Android).unwrap();
    let xpath = xpath_for(&node, MobilePlatform::Android).unwrap();
    assert!(xpath.contains("@class='android.widget.Button'"));
    assert!(xpath.contains("@resource-id='pkg:id/save'"));
    assert!(xpath.contains("@content-desc='Save'"));
}

fn entry() -> RefEntry {
    RefEntry {
        process: RefProcess {
            pid: 1.into(),
            process_instance: Some("instance".into()),
        },
        identity: RefEntryIdentity {
            role: "button".into(),
            name: Some("Save".into()),
            unique_at_capture: true,
            ..Default::default()
        },
        geometry: RefGeometry {
            bounds: None,
            bounds_hash: None,
            bounds_space: None,
        },
        capabilities: RefCapabilities {
            states: Vec::new(),
            available_actions: Vec::new(),
        },
        source: RefSource::default(),
        scope: RefScope {
            root_ref: None,
            path_is_absolute: true,
            path: Default::default(),
        },
    }
}

#[test]
fn full_tree_identity_match_never_uses_bounds_or_values() {
    let tree = super::super::source::parse_source(r#"<hierarchy><node class="android.widget.Button" text="Save" bounds="[0,0][50,50]"/><node class="android.widget.Button" text="Save" bounds="[60,0][100,50]"/></hierarchy>"#, MobilePlatform::Android).unwrap();
    let mut matches = Vec::new();
    let mut entry = entry();
    entry.identity.value = Some("different".into());
    find_matches(&tree, &entry, &mut matches);
    assert_eq!(matches.len(), 2);
}

#[test]
fn mismatched_native_id_does_not_fall_back_to_name() {
    let tree = super::super::source::parse_source(
        r#"<node class="android.widget.Button" text="Save" resource-id="new"/>"#,
        MobilePlatform::Android,
    )
    .unwrap();
    let mut entry = entry();
    entry.identity.native_id = Some(agent_desktop_core::ElementIdentifier {
        kind: agent_desktop_core::IdentifierKind::AutomationId,
        value: "old".into(),
    });
    let mut matches = Vec::new();
    find_matches(&tree, &entry, &mut matches);
    assert!(matches.is_empty());
}

#[test]
fn foreign_native_handle_is_stale() {
    let adapter = MobileAdapter::from_config(crate::MobileConfig {
        platform: MobilePlatform::Ios,
        server_url: "http://localhost:4723".into(),
        session_id: "session".into(),
    })
    .unwrap();
    let handle = NativeHandle::new("foreign");
    let result = adapter.handle(&handle);
    assert_eq!(result.err().unwrap().code, ErrorCode::StaleRef);
}
