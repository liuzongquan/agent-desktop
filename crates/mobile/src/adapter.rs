use crate::system::{config::MobileConfig, connection::Connection};
use agent_desktop_core::{AdapterError, Deadline};

pub struct MobileAdapter {
    pub(crate) connection: Connection,
}

impl MobileAdapter {
    pub fn from_env() -> Result<Option<Self>, AdapterError> {
        MobileConfig::from_env()?.map(Self::from_config).transpose()
    }

    pub fn from_config(config: MobileConfig) -> Result<Self, AdapterError> {
        Ok(Self {
            connection: Connection::new(config)?,
        })
    }

    pub(crate) fn validate(&self, deadline: Deadline) -> Result<(), AdapterError> {
        self.connection.validate_session(deadline)
    }
}
