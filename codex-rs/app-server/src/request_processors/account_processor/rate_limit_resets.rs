//! Legacy RPC shapes remain readable; Lemex never requests or spends reset credits.
use super::*;

impl AccountRequestProcessor {
    pub(super) async fn detailed_rate_limit_reset_credits(
        _client: &BackendClient,
    ) -> Option<RateLimitResetCreditsSummary> {
        None
    }

    pub(crate) async fn consume_account_rate_limit_reset_credit(
        &self,
        _params: ConsumeAccountRateLimitResetCreditParams,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        Err(invalid_request(
            "Credit purchases and resets are unavailable in Lemex.",
        ))
    }
}
