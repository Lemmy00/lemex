mod bedrock;
pub(crate) use bedrock::check_gov_cloud;
mod keys;
pub(crate) mod onboarding_screen;
mod trust_directory;
pub(crate) use crate::terminal_hyperlinks::mark_underlined_hyperlink;
pub(crate) use crate::terminal_hyperlinks::mark_url_hyperlink;
pub(crate) use onboarding_screen::DirectoryTrustOptions;
pub(crate) use trust_directory::TrustCancelAction;

mod provider_setup;
pub(crate) use provider_setup::FieldDisplay;
pub(crate) use provider_setup::ProviderInput;
pub(crate) use provider_setup::move_provider_field;
pub(crate) const SETUP_CONNECTION_LOST: &str =
    "Lemex lost its connection during setup. Restart Lemex to continue.";
