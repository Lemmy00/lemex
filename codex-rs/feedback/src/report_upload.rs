//! Compatibility API for removed external feedback reporting.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::Result;
use codex_http_client::HttpClientFactory;
use codex_protocol::protocol::SessionSource;

use crate::FeedbackAttachment;
use crate::FeedbackSnapshot;
use crate::REPORTING_REMOVED;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FeedbackDelivery {
    Accepted {
        retry_after: Option<Duration>,
    },
    Rejected {
        status: u16,
        retry_after: Option<Duration>,
    },
    Unconfirmed,
}

/// Legacy transport retained so existing clients receive a clear error.
pub struct FeedbackTransport;

impl FeedbackTransport {
    pub fn new(_http_client_factory: HttpClientFactory) -> Result<Self> {
        anyhow::bail!(REPORTING_REMOVED)
    }

    pub async fn send(&self, _envelope: Vec<u8>) -> FeedbackDelivery {
        FeedbackDelivery::Rejected {
            status: 410,
            retry_after: None,
        }
    }
}

impl FeedbackSnapshot {
    pub fn prepare_report_event(
        &self,
        _report_id: &str,
        _classification: &str,
        _reason: Option<&str>,
        _tags: Option<&BTreeMap<String, String>>,
        _session_source: Option<&SessionSource>,
    ) -> Result<Vec<u8>> {
        anyhow::bail!(REPORTING_REMOVED)
    }
}

pub fn prepare_report_attachment(
    _report_id: &str,
    _attachment: FeedbackAttachment,
) -> Result<Vec<u8>> {
    anyhow::bail!(REPORTING_REMOVED)
}
