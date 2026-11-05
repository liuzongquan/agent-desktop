use agent_desktop_core::{AdapterError, ErrorCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MobilePlatform {
    Android,
    Ios,
}

impl MobilePlatform {
    pub fn parse(value: &str) -> Result<Self, AdapterError> {
        match value.to_ascii_lowercase().as_str() {
            "android" => Ok(Self::Android),
            "ios" => Ok(Self::Ios),
            _ => Err(AdapterError::new(
                ErrorCode::InvalidArgs,
                "AGENT_DESKTOP_PLATFORM must be android or ios",
            )),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Android => "android",
            Self::Ios => "ios",
        }
    }
}
