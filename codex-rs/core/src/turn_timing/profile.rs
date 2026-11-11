//! Local turn critical-path diagnostics.
//! Missing capture never becomes a zero-duration measurement.

use serde::Serialize;

/// Capture coverage for one finalized turn, independent of its completion status.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub enum TurnCriticalPath {
    #[default]
    Disabled,
    Incomplete,
    Complete(TurnCriticalPathDurations),
}

impl TurnCriticalPath {
    pub fn status(&self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Incomplete => "incomplete",
            Self::Complete(_) => "complete",
        }
    }

    pub fn durations(&self) -> Option<&TurnCriticalPathDurations> {
        match self {
            Self::Complete(durations) => Some(durations),
            Self::Disabled | Self::Incomplete => None,
        }
    }
}

/// Category totals over the server turn interval, truncated to milliseconds.
/// Inference is the compatibility total of sampling and compaction, not another leaf.
/// Independent truncation can leave the seven leaf categories up to six milliseconds
/// below duration_ms, and sampling_ms + compaction_ms up to one below inference_ms.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TurnCriticalPathDurations {
    pub duration_ms: u64,
    pub inference_ms: u64,
    pub sampling_ms: u64,
    pub compaction_ms: u64,
    pub auto_review_ms: u64,
    /// Portion of auto_review_ms outside observed Guardian turn execution; not additive.
    pub auto_review_overhead_ms: Option<u64>,
    pub user_wait_ms: u64,
    pub tools_ms: u64,
    pub overhead_ms: u64,
    pub unknown_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TurnProfile {
    pub critical_path: TurnCriticalPath,
    pub before_first_sampling_ms: u64,
    pub sampling_ms: u64,
    pub compaction_ms: u64,
    pub between_sampling_overhead_ms: u64,
    pub tool_blocking_ms: u64,
    pub after_last_sampling_ms: u64,
    pub sampling_request_count: u32,
    pub sampling_retry_count: u32,
    pub tools_change_count: u32,
}
