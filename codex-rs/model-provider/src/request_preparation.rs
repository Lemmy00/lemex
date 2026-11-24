//! Provider-specific request preparation and credential checks.

use std::future::Future;
use std::io;

use codex_login::AuthChangeState;
use codex_login::RequestAuthChange;
use codex_model_provider_info::ModelProviderInfo;
use codex_protocol::error::Result;
use codex_protocol::models::ResponseItem;

use crate::ModelProvider;
use crate::rcp_models::is_rcp_kimi_k3_1_pro;

/// Adapt the request's copy of the conversation to the provider's message format.
pub fn adapt_responses_input(provider: &ModelProviderInfo, slug: &str, input: &mut [ResponseItem]) {
    if !provider.is_rcp() || !is_rcp_kimi_k3_1_pro(slug) {
        return;
    }
    // Kimi's RCP deployment accepts instructions in system messages.
    for item in input {
        if let ResponseItem::Message { role, .. } = item
            && role == "developer"
        {
            "system".clone_into(role);
        }
    }
}

/// Prepare a request without dispatching it. When the request uses managed auth,
/// repeat preparation after a credential refresh and fail if the owner changes.
pub async fn prepare_provider_request<T, Fut>(
    provider: &dyn ModelProvider,
    check_auth: bool,
    mut prepare: impl FnMut(Option<AuthChangeState>) -> Fut,
) -> Result<T>
where
    Fut: Future<Output = Result<T>>,
{
    let auth_state = || {
        provider
            .auth_manager()
            .map(|manager| *manager.auth_change_state_receiver().borrow())
    };
    let mut before = auth_state();
    loop {
        let prepared = prepare(before).await?;
        if !check_auth {
            return Ok(prepared);
        }
        // A managed provider can begin or cease exposing its auth manager during setup.
        let after = auth_state();
        let change = match (before, after) {
            (Some(before), Some(after)) => {
                after.compare_for_request(before.owner_generation, before)
            }
            (None, None) => RequestAuthChange::Unchanged,
            _ => RequestAuthChange::OwnerChanged,
        };
        match change {
            RequestAuthChange::Unchanged => return Ok(prepared),
            RequestAuthChange::CredentialsChanged => before = after,
            RequestAuthChange::OwnerChanged => {
                return Err(
                    io::Error::other("account changed while preparing model request").into(),
                );
            }
        }
    }
}

#[cfg(test)]
#[path = "request_preparation_tests.rs"]
mod tests;
