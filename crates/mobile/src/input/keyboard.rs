use crate::{MobileAdapter, MobilePlatform};
use agent_desktop_core::{ActionResult, AdapterError, Deadline, KeyCombo, Modifier};
use serde_json::json;

impl MobileAdapter {
    pub(crate) fn send_key(
        &self,
        combo: &KeyCombo,
        deadline: Deadline,
    ) -> Result<ActionResult, AdapterError> {
        if self.connection.config.platform == MobilePlatform::Android && combo.modifiers.is_empty()
        {
            let code = match combo.key.to_ascii_lowercase().as_str() {
                "back" => Some(4),
                "home" => Some(3),
                "appswitch" => Some(187),
                _ => None,
            };
            if let Some(code) = code {
                self.connection
                    .execute("mobile: pressKey", json!({"keycode":code}), deadline)?;
                return Ok(ActionResult::delivered_unverified("press"));
            }
        }
        let key = webdriver_key(&combo.key)?;
        let modifiers: Vec<&str> = combo
            .modifiers
            .iter()
            .map(|modifier| match modifier {
                Modifier::Shift => "\u{e008}",
                Modifier::Ctrl => "\u{e009}",
                Modifier::Alt => "\u{e00a}",
                Modifier::Meta => "\u{e03d}",
            })
            .collect();
        let mut actions = modifiers
            .iter()
            .map(|value| json!({"type":"keyDown","value":value}))
            .collect::<Vec<_>>();
        actions.extend([
            json!({"type":"keyDown","value":key}),
            json!({"type":"keyUp","value":key}),
        ]);
        actions.extend(
            modifiers
                .iter()
                .rev()
                .map(|value| json!({"type":"keyUp","value":value})),
        );
        self.perform_actions(
            json!({"actions":[{"type":"key","id":"keyboard","actions":actions}]}),
            deadline,
        )?;
        Ok(ActionResult::delivered_unverified("press"))
    }
}

fn webdriver_key(key: &str) -> Result<String, AdapterError> {
    let code = match key.to_ascii_lowercase().as_str() {
        "enter" | "return" => "\u{e007}",
        "tab" => "\u{e004}",
        "backspace" => "\u{e003}",
        "delete" => "\u{e017}",
        "escape" | "esc" => "\u{e00c}",
        "space" => " ",
        "arrowleft" | "left" => "\u{e012}",
        "arrowup" | "up" => "\u{e013}",
        "arrowright" | "right" => "\u{e014}",
        "arrowdown" | "down" => "\u{e015}",
        _ if key.chars().count() == 1 => key,
        _ => return Err(AdapterError::not_supported("requested mobile key")),
    };
    Ok(code.into())
}

#[cfg(test)]
#[path = "keyboard_tests.rs"]
mod tests;
