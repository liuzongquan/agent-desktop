use crate::{MobileAdapter, MobilePlatform};
use agent_desktop_core::{
    AdapterError, ClipboardContent, ClipboardFormat, Deadline, DragParams, InputOps,
    InteractionLease, MouseButton, MouseEvent, MouseEventKind, OperationReport, OperationTarget,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

impl InputOps for MobileAdapter {
    fn keyboard_needs_window_activation(&self) -> bool {
        false
    }

    fn mouse_event(
        &self,
        event: MouseEvent,
        lease: &InteractionLease,
    ) -> Result<OperationReport, AdapterError> {
        if !event.modifiers.is_empty() || !matches!(event.button, MouseButton::Left) {
            return Err(AdapterError::not_supported(
                "mobile pointer modifiers or secondary buttons",
            ));
        }
        event.point.validate()?;
        let MouseEventKind::Click { count } = event.kind else {
            return Err(AdapterError::not_supported(
                "mobile hover, held pointer or wheel; use drag or scroll",
            ));
        };
        agent_desktop_core::validate_mouse_click_count(count)?;
        let mut actions = Vec::new();
        for _ in 0..count {
            actions.extend([json!({"type":"pointerMove","duration":0,"origin":"viewport","x":event.point.x.round() as i64,"y":event.point.y.round() as i64}), json!({"type":"pointerDown","button":0}), json!({"type":"pause","duration":50}), json!({"type":"pointerUp","button":0})]);
        }
        self.touch(actions, lease.deadline())?;
        Ok(OperationReport::delivered_unverified(
            "mouse-click",
            OperationTarget::Pointer { point: event.point },
        ))
    }

    fn drag(
        &self,
        params: DragParams,
        lease: &InteractionLease,
    ) -> Result<OperationReport, AdapterError> {
        params.validate(lease.deadline())?;
        self.touch(vec![
            json!({"type":"pointerMove","duration":0,"origin":"viewport","x":params.from.x.round() as i64,"y":params.from.y.round() as i64}),
            json!({"type":"pointerDown","button":0}), json!({"type":"pause","duration":150}),
            json!({"type":"pointerMove","duration":params.duration_ms.unwrap_or(500),"origin":"viewport","x":params.to.x.round() as i64,"y":params.to.y.round() as i64}),
            json!({"type":"pause","duration":params.drop_delay_ms.unwrap_or(0)}), json!({"type":"pointerUp","button":0}),
        ], lease.deadline())?;
        Ok(OperationReport::delivered_unverified(
            "drag",
            OperationTarget::Drag {
                from: params.from,
                to: params.to,
            },
        ))
    }

    fn get_clipboard_content(
        &self,
        format: ClipboardFormat,
        deadline: Deadline,
    ) -> Result<Option<ClipboardContent>, AdapterError> {
        if !matches!(format, ClipboardFormat::Text | ClipboardFormat::Auto) {
            return Err(AdapterError::not_supported("non-text mobile clipboard"));
        }
        self.validate(deadline)?;
        let value = match self.connection.config.platform {
            MobilePlatform::Android => self.connection.request(
                "POST",
                "/appium/device/get_clipboard",
                Some(json!({"contentType":"plaintext"})),
                deadline,
            )?,
            MobilePlatform::Ios => self.connection.execute(
                "mobile: getClipboard",
                json!({"contentType":"plaintext"}),
                deadline,
            )?,
        };
        let bytes =
            STANDARD
                .decode(value.as_str().ok_or_else(|| {
                    AdapterError::internal("Appium clipboard was not base64 text")
                })?)
                .map_err(|_| AdapterError::internal("Appium clipboard contains invalid base64"))?;
        let text = String::from_utf8(bytes)
            .map_err(|_| AdapterError::internal("Appium clipboard was not UTF-8"))?;
        Ok(Some(ClipboardContent::Text(text)))
    }

    fn set_clipboard_content(
        &self,
        content: &ClipboardContent,
        lease: &InteractionLease,
    ) -> Result<OperationReport, AdapterError> {
        let ClipboardContent::Text(text) = content else {
            return Err(AdapterError::not_supported("non-text mobile clipboard"));
        };
        self.validate(lease.deadline())?;
        let args = json!({"contentType":"plaintext","content":STANDARD.encode(text)});
        match self.connection.config.platform {
            MobilePlatform::Android => {
                self.connection.request(
                    "POST",
                    "/appium/device/set_clipboard",
                    Some(args),
                    lease.deadline(),
                )?;
            }
            MobilePlatform::Ios => {
                self.connection
                    .execute("mobile: setClipboard", args, lease.deadline())?;
            }
        }
        Ok(OperationReport::delivered_unverified(
            "clipboard-set",
            OperationTarget::Clipboard,
        ))
    }

    fn clear_clipboard(&self, lease: &InteractionLease) -> Result<OperationReport, AdapterError> {
        self.set_clipboard_content(&ClipboardContent::Text(String::new()), lease)
    }
}

impl MobileAdapter {
    fn touch(&self, actions: Vec<Value>, deadline: Deadline) -> Result<(), AdapterError> {
        self.perform_actions(json!({"actions":[{"type":"pointer","id":"finger","parameters":{"pointerType":"touch"},"actions":actions}]}), deadline)
    }
}
