use crate::{MobileAdapter, MobilePlatform};
use agent_desktop_core::{
    Action, ActionOps, ActionRequest, ActionResult, AdapterError, Direction, HeadedRequirement,
    InteractionLease, NativeHandle, ObservationOps, RefEntry,
};
use serde_json::json;

impl ActionOps for MobileAdapter {
    fn headed_requirement_for_ref(
        &self,
        _entry: &RefEntry,
        _request: &ActionRequest,
        _deadline: agent_desktop_core::Deadline,
    ) -> Result<HeadedRequirement, AdapterError> {
        Ok(HeadedRequirement::None)
    }
    fn headed_requirement(
        &self,
        _handle: &NativeHandle,
        _request: &ActionRequest,
        _deadline: agent_desktop_core::Deadline,
    ) -> Result<HeadedRequirement, AdapterError> {
        Ok(HeadedRequirement::None)
    }

    fn scroll_into_view(
        &self,
        handle: &NativeHandle,
        lease: &InteractionLease,
    ) -> Result<(), AdapterError> {
        let live = self.get_live_element(handle, lease.deadline())?;
        if live.state.hidden == Some(true) || live.state.offscreen == Some(true) {
            return Err(AdapterError::not_supported(
                "automatic scrolling of hidden mobile elements; scroll the visible container first",
            ));
        }
        Ok(())
    }

    fn execute_action(
        &self,
        handle: &NativeHandle,
        request: ActionRequest,
        lease: &InteractionLease,
    ) -> Result<ActionResult, AdapterError> {
        let deadline = lease.deadline();
        if handle.is_null() {
            if let Action::PressKey(combo) = &request.action {
                self.validate(deadline)?;
                return self.send_key(combo, deadline);
            }
        }
        let live = self.get_live_element(handle, deadline)?;
        if let Some(process) = request.expected_process() {
            self.verify_process(process, deadline)?;
        }
        let target = self.handle(handle)?;
        let prefix = format!("/element/{}", target.id);
        let action = &request.action;
        match action {
            Action::SetFocus if live.state.role != "textfield" => {
                return Err(AdapterError::not_supported(
                    "focus of a non-editable mobile element",
                ));
            }
            Action::Click | Action::SetFocus => {
                self.connection.request(
                    "POST",
                    &format!("{prefix}/click"),
                    Some(json!({})),
                    deadline,
                )?;
            }
            Action::TypeText(text) => {
                self.connection.request(
                    "POST",
                    &format!("{prefix}/value"),
                    Some(json!({"text":text,"value":[text]})),
                    deadline,
                )?;
            }
            Action::SetValue(text) => {
                self.connection.request(
                    "POST",
                    &format!("{prefix}/clear"),
                    Some(json!({})),
                    deadline,
                )?;
                self.connection
                    .request(
                        "POST",
                        &format!("{prefix}/value"),
                        Some(json!({"text":text,"value":[text]})),
                        deadline,
                    )
                    .map_err(|error| {
                        error.with_disposition(agent_desktop_core::DeliverySemantics::uncertain())
                    })?;
            }
            Action::Clear => {
                self.connection.request(
                    "POST",
                    &format!("{prefix}/clear"),
                    Some(json!({})),
                    deadline,
                )?;
            }
            Action::Toggle | Action::Check | Action::Uncheck => {
                if !matches!(
                    live.state.role.as_str(),
                    "checkbox" | "switch" | "radiobutton"
                ) {
                    return Err(AdapterError::not_supported(action.name()));
                }
                let checked = live.state.states.iter().any(|s| s == "checked");
                if matches!(action, Action::Check) && checked
                    || matches!(action, Action::Uncheck) && !checked
                {
                    return Ok(ActionResult::satisfied_without_delivery(action.name())
                        .with_state(live.state));
                }
                self.connection.request(
                    "POST",
                    &format!("{prefix}/click"),
                    Some(json!({})),
                    deadline,
                )?;
            }
            Action::Scroll(direction, amount) => {
                if *amount == 0 {
                    return Ok(ActionResult::satisfied_without_delivery(action.name()));
                }
                let direction = match direction {
                    Direction::Up => "up",
                    Direction::Down => "down",
                    Direction::Left => "left",
                    Direction::Right => "right",
                };
                match self.connection.config.platform {
                    MobilePlatform::Android => {
                        self.connection.execute("mobile: scrollGesture", json!({"elementId":target.id,"direction":direction,"percent": (*amount as f64 / 3.0).clamp(0.1, 1.0)}), deadline)?;
                    }
                    MobilePlatform::Ios => {
                        self.connection.execute(
                            "mobile: scroll",
                            json!({"elementId":target.id,"direction":direction}),
                            deadline,
                        )?;
                    }
                }
            }
            Action::PressKey(combo) => {
                if !live.state.states.iter().any(|state| state == "focused") {
                    return Err(AdapterError::not_supported(
                        "key delivery to an unfocused mobile ref; focus it first",
                    ));
                }
                return self.send_key(combo, deadline);
            }
            Action::DoubleClick => {
                let script = if self.connection.config.platform == MobilePlatform::Android {
                    "mobile: doubleClickGesture"
                } else {
                    "mobile: doubleTap"
                };
                self.connection
                    .execute(script, json!({"elementId":target.id}), deadline)?;
            }
            _ => return Err(AdapterError::not_supported(action.name())),
        }
        Ok(ActionResult::delivered_unverified(action.name()))
    }
}
