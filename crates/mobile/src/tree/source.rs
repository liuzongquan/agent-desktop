use agent_desktop_core::{
    AccessibilityNode, AdapterError, ElementIdentifier, ErrorCode, IdentifierKind, NodeDescriptor,
    NodeIdentity, NodePresentation, Rect,
};
use roxmltree::Node;

use crate::MobilePlatform;

const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_NODES: u32 = 50_000;
const MAX_DEPTH: usize = 50;

pub(crate) fn parse_source(
    xml: &str,
    platform: MobilePlatform,
) -> Result<AccessibilityNode, AdapterError> {
    if xml.len() > MAX_SOURCE_BYTES {
        return Err(source_budget_error());
    }
    let document = roxmltree::Document::parse_with_options(
        xml,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: MAX_NODES,
        },
    )
    .map_err(|error| match error {
        roxmltree::Error::NodesLimitReached => source_budget_error(),
        _ => AdapterError::new(
            ErrorCode::ActionFailed,
            "Appium returned invalid XML source",
        ),
    })?;
    parse_node(document.root_element(), platform, 0)
}

fn source_budget_error() -> AdapterError {
    AdapterError::new(
        ErrorCode::Timeout,
        "Mobile source exceeds the observation budget",
    )
}

fn parse_node(
    node: Node<'_, '_>,
    platform: MobilePlatform,
    depth: usize,
) -> Result<AccessibilityNode, AdapterError> {
    if depth > MAX_DEPTH {
        return Err(source_budget_error());
    }
    let class = node
        .attribute("class")
        .or_else(|| node.attribute("type"))
        .unwrap_or(node.tag_name().name());
    let role = role_for(class, platform);
    let secure = boolean(node, "password") == Some(true) || class.ends_with("SecureTextField");
    let editable = role == "textfield";
    let identity = identity_for(node, platform, editable, secure);
    let mut states = Vec::new();
    for (attribute, expected, state) in [
        ("enabled", false, "disabled"),
        ("focused", true, "focused"),
        ("selected", true, "selected"),
        ("checked", true, "checked"),
        ("visible", false, "hidden"),
        ("displayed", false, "hidden"),
    ] {
        if boolean(node, attribute) == Some(expected) && !states.iter().any(|s| s == state) {
            states.push(state.to_owned());
        }
    }
    if platform == MobilePlatform::Ios
        && matches!(role, "switch" | "checkbox")
        && node.attribute("value") == Some("1")
        && !states.iter().any(|s| s == "checked")
    {
        states.push("checked".into());
    }
    if secure {
        states.push("secure".into());
    }
    let bounds = bounds_for(node, platform)?;
    let mut actions = Vec::new();
    if boolean(node, "clickable") == Some(true)
        || matches!(
            role,
            "button" | "checkbox" | "switch" | "radiobutton" | "cell" | "tab" | "link"
        )
    {
        actions.extend(["Click", "DoubleClick"].map(str::to_owned));
    }
    if editable {
        actions.extend(["Click", "SetValue", "TypeText", "Clear", "SetFocus"].map(str::to_owned));
    }
    if matches!(role, "checkbox" | "switch" | "radiobutton") {
        actions.extend(["Toggle", "Check", "Uncheck"].map(str::to_owned));
    }
    if boolean(node, "scrollable") == Some(true) || matches!(role, "scrollarea" | "table" | "list")
    {
        actions.push("Scroll".into());
    }
    let children = node
        .children()
        .filter(Node::is_element)
        .map(|child| parse_node(child, platform, depth + 1))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(AccessibilityNode {
        ref_id: None,
        role: role.into(),
        identity,
        presentation: NodePresentation {
            states,
            available_actions: actions,
            bounds,
            ref_bounds: bounds,
            descriptors: NodeDescriptor {
                role_description: Some(class.into()),
                placeholder: attribute(node, "hint"),
                ..Default::default()
            },
            ..Default::default()
        },
        children_count: None,
        subtree_truncated: false,
        children,
    })
}

fn identity_for(
    node: Node<'_, '_>,
    platform: MobilePlatform,
    editable: bool,
    secure: bool,
) -> NodeIdentity {
    let (name, value, description, identifier) = match platform {
        MobilePlatform::Android => {
            let description = attribute(node, "content-desc");
            let name = description.clone().or_else(|| {
                (!editable && !secure)
                    .then(|| attribute(node, "text"))
                    .flatten()
            });
            let value = (!secure).then(|| attribute(node, "text")).flatten();
            (name, value, description, attribute(node, "resource-id"))
        }
        MobilePlatform::Ios => (
            attribute(node, "label"),
            (!secure).then(|| attribute(node, "value")).flatten(),
            None,
            attribute(node, "name"),
        ),
    };
    NodeIdentity {
        name,
        value,
        description,
        native_id: identifier.map(|value| ElementIdentifier {
            kind: IdentifierKind::AutomationId,
            value,
        }),
        retained_object: None,
    }
}

fn role_for(class: &str, platform: MobilePlatform) -> &'static str {
    let class = class.rsplit('.').next().unwrap_or(class);
    let class = class.strip_prefix("XCUIElementType").unwrap_or(class);
    match class {
        "hierarchy" | "AppiumAUT" | "Window" => "window",
        "Application" => "application",
        "Button" | "ImageButton" => "button",
        "EditText" | "AutoCompleteTextView" | "TextField" | "SecureTextField" | "SearchField" => {
            "textfield"
        }
        "TextView" if platform == MobilePlatform::Ios => "textfield",
        "TextView" => "statictext",
        "StaticText" => "statictext",
        "CheckBox" | "CheckBoxElement" => "checkbox",
        "Switch" | "ToggleButton" => "switch",
        "RadioButton" => "radiobutton",
        "SeekBar" | "Slider" => "slider",
        "Spinner" | "Picker" | "PickerWheel" => "combobox",
        "ProgressBar" | "ProgressIndicator" | "ActivityIndicator" => "progressbar",
        "ImageView" | "Image" | "Icon" => "image",
        "ScrollView" | "HorizontalScrollView" | "WebView" => "scrollarea",
        "ListView" | "RecyclerView" | "CollectionView" => "list",
        "Table" | "GridView" => "table",
        "Cell" => "cell",
        "Tab" => "tab",
        "TabBar" => "tablist",
        "Toolbar" | "ToolBar" | "NavigationBar" => "toolbar",
        "Alert" => "alert",
        "Sheet" => "sheet",
        "Link" => "link",
        _ => "group",
    }
}

fn attribute(node: Node<'_, '_>, name: &str) -> Option<String> {
    node.attribute(name)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
}

fn boolean(node: Node<'_, '_>, name: &str) -> Option<bool> {
    match node.attribute(name) {
        Some("true" | "1") => Some(true),
        Some("false" | "0") => Some(false),
        _ => None,
    }
}

fn bounds_for(node: Node<'_, '_>, platform: MobilePlatform) -> Result<Option<Rect>, AdapterError> {
    let bounds = match platform {
        MobilePlatform::Android => {
            let Some(raw) = node.attribute("bounds") else {
                return Ok(None);
            };
            let parts = raw
                .split(['[', ']', ','])
                .filter(|part| !part.is_empty())
                .map(str::parse::<f64>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| invalid_bounds())?;
            let [x, y, right, bottom] = parts.as_slice() else {
                return Err(invalid_bounds());
            };
            Rect {
                x: *x,
                y: *y,
                width: right - x,
                height: bottom - y,
            }
        }
        MobilePlatform::Ios => {
            if ["x", "y", "width", "height"]
                .iter()
                .all(|key| node.attribute(*key).is_none())
            {
                return Ok(None);
            }
            let read = |key| {
                node.attribute(key)
                    .and_then(|value| value.parse::<f64>().ok())
                    .ok_or_else(invalid_bounds)
            };
            Rect {
                x: read("x")?,
                y: read("y")?,
                width: read("width")?,
                height: read("height")?,
            }
        }
    };
    bounds.validate().map(Some).map_err(|_| invalid_bounds())
}

fn invalid_bounds() -> AdapterError {
    AdapterError::new(
        ErrorCode::ActionFailed,
        "Appium source contains invalid element bounds",
    )
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
