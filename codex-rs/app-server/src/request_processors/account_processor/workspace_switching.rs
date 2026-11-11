//! Compatibility responses for account-session RPCs in the API-key Lemex client.

use super::*;
use codex_app_server_protocol::AccountSessionsListParams;
use codex_app_server_protocol::AccountSessionsResponse;
use codex_app_server_protocol::AccountSessionsSwitchParams;
use codex_app_server_protocol::AccountSessionsSwitchResponse;

impl AccountRequestProcessor {
    pub(crate) async fn list_account_sessions(
        &self,
        _params: AccountSessionsListParams,
    ) -> Result<AccountSessionsResponse, JSONRPCErrorError> {
        Ok(AccountSessionsResponse {
            active_session_id: None,
            sessions: Vec::new(),
        })
    }

    pub(crate) async fn switch_account_session(
        &self,
        _params: AccountSessionsSwitchParams,
        _threads: &crate::request_processors::ThreadRequestProcessor,
        _admission: &crate::turn_admission::TurnAdmission,
    ) -> Result<AccountSessionsSwitchResponse, JSONRPCErrorError> {
        Err(invalid_request(
            "ChatGPT account switching is unavailable in Lemex. Use `lemex setup` to configure an API provider.",
        ))
    }
}
