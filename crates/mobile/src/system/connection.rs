use super::config::MobileConfig;
use agent_desktop_core::{AdapterError, Deadline, DeliverySemantics, ErrorCode};
use serde_json::{Value, json};
use std::io::Read;

const MAX_RESPONSE: u64 = 24 * 1024 * 1024;

pub(crate) struct Connection {
    pub(crate) config: MobileConfig,
    agent: ureq::Agent,
}

impl Connection {
    pub(crate) fn new(config: MobileConfig) -> Result<Self, AdapterError> {
        config.validate()?;
        Ok(Self {
            config,
            agent: ureq::AgentBuilder::new().redirects(0).build(),
        })
    }

    pub(crate) fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<Value>,
        deadline: Deadline,
    ) -> Result<Value, AdapterError> {
        deadline.ensure_remaining()?;
        let url = format!(
            "{}/session/{}{}",
            self.config.server_url.trim_end_matches('/'),
            self.config.session_id,
            path
        );
        let req = self
            .agent
            .request(method, &url)
            .timeout(deadline.remaining());
        let response = match body {
            Some(body) => req.send_json(body),
            None => req.call(),
        };
        let response = match response {
            Ok(response) => response,
            Err(ureq::Error::Status(_, response)) => response,
            Err(_) => return Err(transport_error(method, deadline)),
        };
        let status = response.status();
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(MAX_RESPONSE + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| transport_error(method, deadline))?;
        if bytes.len() as u64 > MAX_RESPONSE {
            return Err(protocol_error("Appium response exceeds 24 MiB", method));
        }
        let json: Value = serde_json::from_slice(&bytes)
            .map_err(|_| protocol_error("Appium returned invalid JSON", method))?;
        let value = json
            .get("value")
            .ok_or_else(|| protocol_error("Appium response has no W3C value", method))?;
        if let Some(error) = value.get("error").and_then(Value::as_str) {
            let code = match error {
                "stale element reference" | "no such element" | "invalid session id" => {
                    ErrorCode::StaleRef
                }
                "timeout" | "script timeout" => ErrorCode::Timeout,
                "invalid argument" => ErrorCode::InvalidArgs,
                "unknown command" | "unsupported operation" => ErrorCode::ActionNotSupported,
                _ => ErrorCode::ActionFailed,
            };
            let disposition = if matches!(
                code,
                ErrorCode::StaleRef | ErrorCode::InvalidArgs | ErrorCode::ActionNotSupported
            ) || method == "GET"
            {
                DeliverySemantics::not_delivered()
            } else {
                DeliverySemantics::uncertain()
            };
            return Err(
                AdapterError::new(code, format!("Appium command failed: {error}"))
                    .with_disposition(disposition),
            );
        }
        if !(200..300).contains(&status) {
            return Err(protocol_error(
                "Appium returned an unsuccessful HTTP status",
                method,
            ));
        }
        deadline.ensure_remaining().map_err(|err| {
            if method == "GET" {
                err.with_disposition(DeliverySemantics::not_delivered())
            } else {
                err.with_disposition(DeliverySemantics::uncertain())
            }
        })?;
        Ok(value.clone())
    }

    pub(crate) fn execute(
        &self,
        script: &str,
        args: Value,
        deadline: Deadline,
    ) -> Result<Value, AdapterError> {
        self.request(
            "POST",
            "/execute/sync",
            Some(json!({"script":script,"args":[args]})),
            deadline,
        )
    }

    pub(crate) fn validate_session(&self, deadline: Deadline) -> Result<(), AdapterError> {
        let value = self.request("GET", "", None, deadline)?;
        let caps = value.get("capabilities").unwrap_or(&value);
        let actual = caps
            .get("platformName")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !actual.eq_ignore_ascii_case(self.config.platform.name()) {
            return Err(AdapterError::new(
                ErrorCode::InvalidArgs,
                "Appium session platform does not match AGENT_DESKTOP_PLATFORM",
            ));
        }
        let context = self.request("GET", "/context", None, deadline)?;
        if context.as_str() != Some("NATIVE_APP") {
            return Err(AdapterError::new(
                ErrorCode::ActionNotSupported,
                "Mobile adapter requires Appium NATIVE_APP context",
            ));
        }
        Ok(())
    }
}

fn protocol_error(message: &str, method: &str) -> AdapterError {
    AdapterError::new(ErrorCode::ActionFailed, message).with_disposition(if method == "GET" {
        DeliverySemantics::not_delivered()
    } else {
        DeliverySemantics::uncertain()
    })
}

fn transport_error(method: &str, deadline: Deadline) -> AdapterError {
    let code = if deadline.remaining().is_zero() {
        ErrorCode::Timeout
    } else {
        ErrorCode::AppUnresponsive
    };
    AdapterError::new(code, "Appium connection failed or timed out").with_disposition(
        if method == "GET" {
            DeliverySemantics::not_delivered()
        } else {
            DeliverySemantics::uncertain()
        },
    )
}

#[cfg(test)]
#[path = "connection_tests.rs"]
mod tests;
