use crate::{MobileAdapter, MobilePlatform};
use agent_desktop_core::{AccessibilityNode, AdapterError, Deadline, NativeHandle, RefEntry};
use serde_json::{Value, json};

pub(crate) struct ElementHandle {
    pub(crate) id: String,
    pub(crate) entry: RefEntry,
}

impl MobileAdapter {
    pub(crate) fn read_source(
        &self,
        deadline: Deadline,
    ) -> Result<AccessibilityNode, AdapterError> {
        self.validate(deadline)?;
        let value = self.connection.request("GET", "/source", None, deadline)?;
        let source = value
            .as_str()
            .ok_or_else(|| AdapterError::internal("Appium source was not XML text"))?;
        let tree = super::source::parse_source(source, self.connection.config.platform)?;
        deadline.ensure_remaining()?;
        Ok(tree)
    }

    pub(crate) fn entry_node(
        &self,
        entry: &RefEntry,
        deadline: Deadline,
    ) -> Result<AccessibilityNode, AdapterError> {
        let expected = entry
            .process
            .identity()
            .ok_or_else(|| AdapterError::stale_ref_because("Mobile ref has no session identity"))?;
        let app = self.verify_process(&expected, deadline)?;
        let window_id = format!("mobile:{}", app.process_instance.as_deref().unwrap_or(""));
        if entry.source.source_window_id.as_deref() != Some(&window_id) {
            return Err(AdapterError::stale_ref_because(
                "Mobile ref belongs to a different window",
            ));
        }
        let tree = self.read_source(deadline)?;
        let mut matches = Vec::new();
        find_matches(&tree, entry, &mut matches);
        self.verify_process(&expected, deadline)?;
        match matches.as_slice() {
            [] => Err(AdapterError::stale_ref_because(
                "Mobile element identity no longer exists",
            )),
            [node] => Ok((*node).clone()),
            _ => Err(AdapterError::ambiguous_target(
                "Mobile element identity is not unique",
            )),
        }
    }

    pub(crate) fn resolve_entry(
        &self,
        entry: &RefEntry,
        deadline: Deadline,
    ) -> Result<NativeHandle, AdapterError> {
        let node = self.entry_node(entry, deadline)?;
        let xpath = xpath_for(&node, self.connection.config.platform)?;
        let elements = self.connection.request(
            "POST",
            "/elements",
            Some(json!({"using":"xpath","value":xpath})),
            deadline,
        )?;
        let elements = elements
            .as_array()
            .ok_or_else(|| AdapterError::internal("Appium element search was not an array"))?;
        let element = match elements.as_slice() {
            [] => {
                return Err(AdapterError::stale_ref_because(
                    "Mobile element disappeared during resolution",
                ));
            }
            [element] => element,
            _ => {
                return Err(AdapterError::ambiguous_target(
                    "Mobile locator matched multiple elements",
                ));
            }
        };
        let id = element
            .get("element-6066-11e4-a52e-4f735466cecf")
            .and_then(Value::as_str)
            .filter(|id| {
                !id.is_empty()
                    && id
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
            })
            .ok_or_else(|| AdapterError::internal("Appium returned an invalid element id"))?;
        Ok(NativeHandle::new(ElementHandle {
            id: id.into(),
            entry: entry.clone(),
        }))
    }

    pub(crate) fn handle<'a>(
        &self,
        handle: &'a NativeHandle,
    ) -> Result<&'a ElementHandle, AdapterError> {
        handle
            .downcast_ref::<ElementHandle>()
            .ok_or_else(|| AdapterError::stale_ref_because("Handle is not a mobile element"))
    }

    pub(crate) fn handle_node(
        &self,
        handle: &NativeHandle,
        deadline: Deadline,
    ) -> Result<AccessibilityNode, AdapterError> {
        let handle = self.handle(handle)?;
        let enabled = self.connection.request(
            "GET",
            &format!("/element/{}/enabled", handle.id),
            None,
            deadline,
        )?;
        let enabled = enabled.as_bool().ok_or_else(|| {
            AdapterError::internal("Appium element enabled state was not a boolean")
        })?;
        let mut node = self.entry_node(&handle.entry, deadline)?;
        self.confirm_handle_identity(handle, &node, deadline)?;
        let process =
            handle.entry.process.identity().ok_or_else(|| {
                AdapterError::stale_ref_because("Mobile ref has no session identity")
            })?;
        self.verify_process(&process, deadline)?;
        node.presentation.states.retain(|state| state != "disabled");
        if !enabled {
            node.presentation.states.push("disabled".into());
        }
        Ok(node)
    }

    fn confirm_handle_identity(
        &self,
        handle: &ElementHandle,
        node: &AccessibilityNode,
        deadline: Deadline,
    ) -> Result<(), AdapterError> {
        let android = self.connection.config.platform == MobilePlatform::Android;
        let native_type =
            self.element_attribute(handle, if android { "class" } else { "type" }, deadline)?;
        if native_type != node.presentation.descriptors.role_description {
            return Err(changed_handle());
        }
        if let Some(identifier) = &handle.entry.identity.native_id {
            let native_id = self.element_attribute(
                handle,
                if android { "resource-id" } else { "name" },
                deadline,
            )?;
            if native_id.as_deref() != Some(&identifier.value) {
                return Err(changed_handle());
            }
        }
        let description = if android {
            self.element_attribute(handle, "content-desc", deadline)?
        } else {
            None
        };
        let name = if android {
            match description.clone() {
                Some(description) => Some(description),
                None if node.role != "textfield"
                    && !node
                        .presentation
                        .states
                        .iter()
                        .any(|state| state == "secure") =>
                {
                    self.element_attribute(handle, "text", deadline)?
                }
                None => None,
            }
        } else {
            self.element_attribute(handle, "label", deadline)?
        };
        let identity = &handle.entry.identity;
        if identity
            .name
            .as_ref()
            .is_some_and(|expected| name.as_ref() != Some(expected))
            || identity
                .description
                .as_ref()
                .is_some_and(|expected| description.as_ref() != Some(expected))
        {
            return Err(changed_handle());
        }
        Ok(())
    }

    fn element_attribute(
        &self,
        handle: &ElementHandle,
        name: &str,
        deadline: Deadline,
    ) -> Result<Option<String>, AdapterError> {
        let value = self.connection.request(
            "GET",
            &format!("/element/{}/attribute/{name}", handle.id),
            None,
            deadline,
        )?;
        match value {
            Value::Null => Ok(None),
            Value::String(value) if value.trim().is_empty() => Ok(None),
            Value::String(value) => Ok(Some(value)),
            _ => Err(AdapterError::internal(
                "Appium identity attribute was not text",
            )),
        }
    }
}

fn changed_handle() -> AdapterError {
    AdapterError::stale_ref_because("Appium element handle no longer matches the captured identity")
}

fn find_matches<'a>(
    node: &'a AccessibilityNode,
    entry: &RefEntry,
    matches: &mut Vec<&'a AccessibilityNode>,
) {
    let id = &entry.identity;
    if node.role == id.role
        && id
            .name
            .as_ref()
            .is_none_or(|name| node.identity.name.as_ref() == Some(name))
        && id
            .description
            .as_ref()
            .is_none_or(|description| node.identity.description.as_ref() == Some(description))
        && id
            .native_id
            .as_ref()
            .is_none_or(|native_id| node.identity.native_id.as_ref() == Some(native_id))
    {
        matches.push(node);
    }
    for child in &node.children {
        find_matches(child, entry, matches);
    }
}

fn xpath_for(node: &AccessibilityNode, platform: MobilePlatform) -> Result<String, AdapterError> {
    let native_type = node
        .presentation
        .descriptors
        .role_description
        .as_deref()
        .ok_or_else(|| AdapterError::stale_ref_because("Mobile node has no native class"))?;
    let type_attr = if platform == MobilePlatform::Android {
        "class"
    } else {
        "type"
    };
    let mut predicates = vec![format!("@{type_attr}={}", xpath_literal(native_type))];
    if let Some(id) = &node.identity.native_id {
        let attr = if platform == MobilePlatform::Android {
            "resource-id"
        } else {
            "name"
        };
        predicates.push(format!("@{attr}={}", xpath_literal(&id.value)));
    }
    if let Some(name) = &node.identity.name {
        let escaped = xpath_literal(name);
        predicates.push(match platform {
            MobilePlatform::Android => format!("(@content-desc={escaped} or @text={escaped})"),
            MobilePlatform::Ios => format!("(@label={escaped} or @name={escaped})"),
        });
    }
    Ok(format!("//*[{}]", predicates.join(" and ")))
}

fn xpath_literal(value: &str) -> String {
    if !value.contains('\'') {
        return format!("'{value}'");
    }
    if !value.contains('"') {
        return format!("\"{value}\"");
    }
    format!(
        "concat({})",
        value
            .split('\'')
            .map(|part| format!("'{part}'"))
            .collect::<Vec<_>>()
            .join(",\"'\",")
    )
}

#[cfg(test)]
#[path = "resolve_tests.rs"]
mod tests;
