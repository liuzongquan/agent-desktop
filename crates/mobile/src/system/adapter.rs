use crate::{MobileAdapter, MobilePlatform};
use agent_desktop_core::system::{
    launch_options::LaunchOptions, launch_result::LaunchResult, process_state::ProcessState,
};
use agent_desktop_core::{
    AdapterError, Deadline, DisplayInfo, ImageBuffer, ImageFormat, InteractionLease,
    OperationReport, OperationTarget, PermissionReport, PermissionState, ProcessIdentity,
    ScreenshotTarget, SnapshotSurface, SystemOps, WindowInfo,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::json;

impl SystemOps for MobileAdapter {
    fn platform_name(&self) -> &'static str {
        self.connection.config.platform.name()
    }

    fn acquire_interaction_lease(
        &self,
        deadline: Deadline,
    ) -> Result<InteractionLease, AdapterError> {
        #[cfg(unix)]
        {
            agent_desktop_core::acquire_unix_interaction_lease(deadline)
        }
        #[cfg(not(unix))]
        {
            let guard = agent_desktop_core::ProcessLeaseGuard::acquire(deadline)?;
            let path = std::env::temp_dir().join("agent-desktop-mobile-interaction.lock");
            let lock =
                agent_desktop_core::FileLock::acquire(&path, deadline, "mobile interaction")?;
            InteractionLease::guarded(deadline, (guard, lock))
        }
    }

    fn permission_report(&self, deadline: Deadline) -> Result<PermissionReport, AdapterError> {
        self.validate(deadline)?;
        Ok(PermissionReport {
            accessibility: PermissionState::Granted,
            screen_recording: PermissionState::Granted,
            automation: PermissionState::Granted,
        })
    }

    fn supported_surfaces(&self) -> Vec<SnapshotSurface> {
        vec![SnapshotSurface::Window]
    }
    fn focused_window(&self, deadline: Deadline) -> Result<Option<WindowInfo>, AdapterError> {
        self.window(deadline).map(Some)
    }
    fn resolve_window_strict(
        &self,
        win: &WindowInfo,
        deadline: Deadline,
    ) -> Result<WindowInfo, AdapterError> {
        self.verify_window(win, deadline)
    }
    fn focus_window(&self, win: &WindowInfo, lease: &InteractionLease) -> Result<(), AdapterError> {
        self.verify_window(win, lease.deadline()).map(|_| ())
    }
    fn focus_app(
        &self,
        process: ProcessIdentity,
        lease: &InteractionLease,
    ) -> Result<(), AdapterError> {
        self.verify_process(&process, lease.deadline()).map(|_| ())
    }

    fn press_key_for_app(
        &self,
        process: ProcessIdentity,
        combo: &agent_desktop_core::KeyCombo,
        _policy: agent_desktop_core::InteractionPolicy,
        lease: &InteractionLease,
    ) -> Result<agent_desktop_core::ActionResult, AdapterError> {
        self.verify_process(&process, lease.deadline())?;
        self.send_key(combo, lease.deadline())
    }

    fn process_state(
        &self,
        process: ProcessIdentity,
        deadline: Deadline,
    ) -> Result<ProcessState, AdapterError> {
        self.verify_process(&process, deadline)?;
        Ok(ProcessState::Running)
    }

    fn launch_app(
        &self,
        id: &str,
        options: &LaunchOptions,
        lease: &InteractionLease,
    ) -> Result<LaunchResult, AdapterError> {
        if !options.args.is_empty()
            || !options.env.is_empty()
            || options.cwd.is_some()
            || options.cdp_port.is_some()
        {
            return Err(AdapterError::not_supported(
                "mobile launch arguments, environment, directory or CDP",
            ));
        }
        self.validate(lease.deadline())?;
        let current = self.current_app(lease.deadline())?;
        let args = match self.connection.config.platform {
            MobilePlatform::Android => json!({"appId":id}),
            MobilePlatform::Ios => json!({"bundleId":id}),
        };
        if !options.attach_if_running {
            let state =
                self.connection
                    .execute("mobile: queryAppState", args.clone(), lease.deadline())?;
            let state = state
                .as_u64()
                .ok_or_else(|| AdapterError::internal("Appium returned invalid app state"))?;
            if state >= 2 {
                return Err(AdapterError::new(
                    agent_desktop_core::ErrorCode::ActionFailed,
                    "Application is already running",
                ));
            }
        }
        if !current.matches_identifier(id) {
            if !options.activate {
                return Err(AdapterError::not_supported(
                    "background mobile launch; use --activate to open the app",
                ));
            }
            self.connection
                .execute("mobile: activateApp", args, lease.deadline())?;
        }
        let window = self.window(lease.deadline()).map_err(|err| {
            err.with_disposition(agent_desktop_core::DeliverySemantics::uncertain())
        })?;
        if window.app != id {
            return Err(AdapterError::new(
                agent_desktop_core::ErrorCode::ActionFailed,
                "Launched app did not become foreground",
            )
            .with_disposition(agent_desktop_core::DeliverySemantics::delivered_unverified()));
        }
        Ok(LaunchResult {
            app: id.into(),
            pid: window.pid,
            process_instance: window.process_instance.clone(),
            window: Some(window),
            cdp: None,
            renderer: None,
            suggestion: None,
        })
    }

    fn close_app(
        &self,
        app: &agent_desktop_core::AppInfo,
        force: bool,
        lease: &InteractionLease,
    ) -> Result<OperationReport, AdapterError> {
        if !force {
            return Err(AdapterError::not_supported(
                "graceful mobile close; use --force to terminate",
            ));
        }
        let instance = app
            .process_instance
            .as_deref()
            .ok_or_else(|| AdapterError::stale_ref_because("App has no mobile session identity"))?;
        self.verify_process(&ProcessIdentity::new(app.pid, instance), lease.deadline())?;
        let id = app
            .bundle_id
            .as_deref()
            .ok_or_else(|| AdapterError::stale_ref_because("App has no bundle/package id"))?;
        let args = match self.connection.config.platform {
            MobilePlatform::Android => json!({"appId":id}),
            MobilePlatform::Ios => json!({"bundleId":id}),
        };
        self.connection
            .execute("mobile: terminateApp", args, lease.deadline())?;
        Ok(OperationReport::delivered_unverified(
            "close-app",
            OperationTarget::for_app(app)?,
        ))
    }

    fn screenshot(
        &self,
        target: ScreenshotTarget,
        deadline: Deadline,
    ) -> Result<ImageBuffer, AdapterError> {
        let expected_window = match &target {
            ScreenshotTarget::ExactWindow(window) => Some(window.clone()),
            _ => None,
        };
        match target {
            ScreenshotTarget::FullScreen | ScreenshotTarget::Screen(0) => {}
            ScreenshotTarget::Display { index: 0, expected } => {
                let actual = self.list_displays(deadline)?;
                if actual.first() != Some(&expected) {
                    return Err(AdapterError::stale_ref_because("Mobile display changed"));
                }
            }
            ScreenshotTarget::ExactWindow(window) => {
                self.verify_window(&window, deadline)?;
            }
            _ => return Err(AdapterError::not_supported("additional mobile displays")),
        }
        self.validate(deadline)?;
        let bounds = self
            .window(deadline)?
            .bounds
            .ok_or_else(|| AdapterError::internal("Mobile display has no bounds"))?;
        let value = self
            .connection
            .request("GET", "/screenshot", None, deadline)?;
        let bytes = STANDARD
            .decode(
                value
                    .as_str()
                    .ok_or_else(|| AdapterError::internal("Appium screenshot was not base64"))?,
            )
            .map_err(|_| AdapterError::internal("Appium returned invalid base64 screenshot"))?;
        let (width, height) = agent_desktop_core::parse_png_dimensions(&bytes)
            .ok_or_else(|| AdapterError::internal("Appium returned invalid PNG screenshot"))?;
        let scale_factor = f64::from(width) / bounds.width;
        if !scale_factor.is_finite()
            || scale_factor <= 0.0
            || (f64::from(height) - bounds.height * scale_factor).abs() > scale_factor
        {
            return Err(AdapterError::new(
                agent_desktop_core::ErrorCode::ActionFailed,
                "Mobile screen rotated or changed size during screenshot; retry",
            ));
        }
        if let Some(window) = expected_window {
            self.verify_window(&window, deadline)?;
        }
        deadline.ensure_remaining()?;
        Ok(ImageBuffer {
            data: bytes,
            format: ImageFormat::Png,
            width,
            height,
            scale_factor,
        })
    }

    fn screenshot_window_frame(
        &self,
        window: &WindowInfo,
        deadline: Deadline,
    ) -> Result<ImageBuffer, AdapterError> {
        self.screenshot(ScreenshotTarget::ExactWindow(window.clone()), deadline)
    }

    fn list_displays(&self, deadline: Deadline) -> Result<Vec<DisplayInfo>, AdapterError> {
        let window = self.window(deadline)?;
        Ok(vec![DisplayInfo {
            id: format!("mobile-display:{}", self.connection.config.session_id),
            bounds: window
                .bounds
                .ok_or_else(|| AdapterError::internal("Mobile display has no bounds"))?,
            is_primary: true,
            scale: 1.0,
        }])
    }
}
