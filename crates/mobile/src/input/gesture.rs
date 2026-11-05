use crate::MobileAdapter;
use agent_desktop_core::{AdapterError, Deadline, DeliverySemantics};
use serde_json::Value;

impl MobileAdapter {
    pub(crate) fn perform_actions(
        &self,
        actions: Value,
        deadline: Deadline,
    ) -> Result<(), AdapterError> {
        self.validate(deadline)?;
        let result = self
            .connection
            .request("POST", "/actions", Some(actions), deadline);
        if result.is_err() {
            if let Ok(cleanup) = Deadline::detached_after(500) {
                let _ = self.connection.request("DELETE", "/actions", None, cleanup);
            }
        }
        result
            .map(|_| ())
            .map_err(|error| error.with_disposition(DeliverySemantics::uncertain()))
    }
}
