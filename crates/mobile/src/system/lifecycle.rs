use crate::{MobileAdapter, MobilePlatform};
use agent_desktop_core::{
    AdapterError, AppInfo, AppPresentation, Deadline, ProcessId, ProcessIdentity, Rect, WindowInfo,
    WindowState,
};
use serde_json::{Value, json};
use std::hash::{Hash, Hasher};

impl MobileAdapter {
    pub(crate) fn current_app(&self, deadline: Deadline) -> Result<AppInfo, AdapterError> {
        self.validate(deadline)?;
        let id = match self.connection.config.platform {
            MobilePlatform::Android => {
                self.connection
                    .request("GET", "/appium/device/current_package", None, deadline)?
            }
            MobilePlatform::Ios => self
                .connection
                .execute("mobile: activeAppInfo", json!({}), deadline)?
                .get("bundleId")
                .cloned()
                .unwrap_or(Value::Null),
        };
        let id = id.as_str().filter(|id| !id.is_empty()).ok_or_else(|| {
            AdapterError::internal("Appium did not report a foreground application")
        })?;
        Ok(self.app_info(id))
    }

    pub(crate) fn app_info(&self, id: &str) -> AppInfo {
        let config = &self.connection.config;
        let instance = format!(
            "appium:{}:{}:{}:{}",
            config.platform.name(),
            config.server_url.trim_end_matches('/'),
            config.session_id,
            id
        );
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        instance.hash(&mut hash);
        AppInfo {
            name: id.into(),
            pid: ProcessId::new((hash.finish() as u32).max(1)),
            bundle_id: Some(id.into()),
            process_instance: Some(instance),
            presentation: Some(AppPresentation::Foreground),
        }
    }

    pub(crate) fn window(&self, deadline: Deadline) -> Result<WindowInfo, AdapterError> {
        let app = self.current_app(deadline)?;
        let rect = self
            .connection
            .request("GET", "/window/rect", None, deadline)?;
        Ok(WindowInfo {
            id: format!("mobile:{}", app.process_instance.as_deref().unwrap_or("")),
            title: app.name.clone(),
            app: app.name,
            pid: app.pid,
            process_instance: app.process_instance,
            bounds: Some(parse_rect(&rect)?),
            state: WindowState {
                is_focused: true,
                accessible: true,
                minimized: Some(false),
                visible: Some(true),
            },
        })
    }

    pub(crate) fn verify_process(
        &self,
        expected: &ProcessIdentity,
        deadline: Deadline,
    ) -> Result<AppInfo, AdapterError> {
        let app = self.current_app(deadline)?;
        if app.pid != expected.pid || app.process_instance.as_deref() != Some(&expected.instance) {
            return Err(AdapterError::stale_ref_because(
                "Mobile session or foreground application changed",
            ));
        }
        Ok(app)
    }

    pub(crate) fn verify_window(
        &self,
        expected: &WindowInfo,
        deadline: Deadline,
    ) -> Result<WindowInfo, AdapterError> {
        let actual = self.window(deadline)?;
        if actual.id != expected.id
            || actual.pid != expected.pid
            || actual.process_instance != expected.process_instance
        {
            return Err(AdapterError::stale_ref_because(
                "Mobile session or foreground window changed",
            ));
        }
        Ok(actual)
    }
}

pub(crate) fn parse_rect(value: &Value) -> Result<Rect, AdapterError> {
    let number = |key| {
        value
            .get(key)
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
            .ok_or_else(|| AdapterError::internal("Appium returned invalid element geometry"))
    };
    let rect = Rect {
        x: number("x")?,
        y: number("y")?,
        width: number("width")?,
        height: number("height")?,
    };
    if rect.width < 0.0 || rect.height < 0.0 {
        return Err(AdapterError::internal("Appium returned negative geometry"));
    }
    Ok(rect)
}
