use agent_desktop_core::{AdapterError, ErrorCode};

use super::platform::MobilePlatform;

#[derive(Debug, Clone)]
pub struct MobileConfig {
    pub platform: MobilePlatform,
    pub server_url: String,
    pub session_id: String,
}

impl MobileConfig {
    pub fn from_env() -> Result<Option<Self>, AdapterError> {
        let platform = match std::env::var("AGENT_DESKTOP_PLATFORM") {
            Err(std::env::VarError::NotPresent) => return Ok(None),
            Err(_) => return Err(invalid("AGENT_DESKTOP_PLATFORM must be Unicode")),
            Ok(value) if value == "desktop" => return Ok(None),
            Ok(value) => MobilePlatform::parse(&value)?,
        };
        let config = Self {
            platform,
            server_url: match std::env::var("AGENT_DESKTOP_APPIUM_URL") {
                Ok(value) => value,
                Err(std::env::VarError::NotPresent) => "http://127.0.0.1:4723".into(),
                Err(_) => return Err(invalid("AGENT_DESKTOP_APPIUM_URL must be Unicode")),
            },
            session_id: std::env::var("AGENT_DESKTOP_APPIUM_SESSION_ID").map_err(|_| {
                invalid(
                    "AGENT_DESKTOP_APPIUM_SESSION_ID must name an existing Appium native session",
                )
            })?,
        };
        config.validate()?;
        Ok(Some(config))
    }

    pub(crate) fn validate(&self) -> Result<(), AdapterError> {
        if matches!(self.session_id.as_str(), "" | "." | "..")
            || !self
                .session_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
            || self.session_id.len() > 256
        {
            return Err(invalid(
                "Appium session id must contain only letters, digits, '-', '_' or '.'",
            ));
        }
        let url = &self.server_url;
        if !(url.starts_with("http://") || url.starts_with("https://"))
            || url.contains(['?', '#', '@'])
            || url.bytes().any(|b| b.is_ascii_whitespace())
        {
            return Err(invalid(
                "Appium URL must be HTTP(S), without credentials, query or fragment",
            ));
        }
        let authority = url
            .split("://")
            .nth(1)
            .unwrap_or("")
            .split('/')
            .next()
            .unwrap_or("");
        if authority.is_empty() {
            return Err(invalid("Appium URL requires a host"));
        }
        Ok(())
    }
}

fn invalid(message: &str) -> AdapterError {
    AdapterError::new(ErrorCode::InvalidArgs, message)
}

#[cfg(test)]
#[path = "config_tests.rs"]
mod tests;
