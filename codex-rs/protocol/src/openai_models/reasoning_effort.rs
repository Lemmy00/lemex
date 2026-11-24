//! Model-owned effort normalization shared by native requests and configuration update items.
//!
//! UI aliases resolve through the same model defaults and fallbacks in both paths.

use super::ModelInfo;
use super::ReasoningEffort;

impl ReasoningEffort {
    pub fn enables_reasoning(&self) -> bool {
        !matches!(self.as_str(), "none" | "persistent" | "disabled" | "off")
    }
}

impl ModelInfo {
    /// Prefer an enabled catalog default, then medium or the first advertised enabled effort.
    pub fn preferred_reasoning_effort(&self) -> Option<&ReasoningEffort> {
        let declared = self.default_reasoning_level.as_ref().filter(|effort| {
            self.supported_reasoning_levels.is_empty()
                || self
                    .supported_reasoning_levels
                    .iter()
                    .any(|option| &option.effort == *effort)
        });
        if declared.is_some_and(ReasoningEffort::enables_reasoning) {
            return declared;
        }

        let mut enabled = self
            .supported_reasoning_levels
            .iter()
            .map(|option| &option.effort)
            .filter(|effort| effort.enables_reasoning() && **effort != ReasoningEffort::Ultra);
        enabled
            .clone()
            .find(|effort| **effort == ReasoningEffort::Medium)
            .or_else(|| enabled.next())
            .or(declared)
    }

    /// Resolves a selected effort to the value sent in an ordinary inference request.
    pub fn resolve_reasoning_effort(&self, effort: ReasoningEffort) -> ReasoningEffort {
        match effort {
            ReasoningEffort::Ultra => self
                .multi_agent_reasoning_effort
                .as_ref()
                .filter(|effort| {
                    *effort != &ReasoningEffort::Ultra
                        && self
                            .supported_reasoning_levels
                            .iter()
                            .any(|preset| &preset.effort == *effort)
                })
                .cloned()
                .or_else(|| {
                    self.supported_reasoning_levels
                        .iter()
                        .find(|preset| preset.effort == ReasoningEffort::Max)
                        .or_else(|| {
                            self.supported_reasoning_levels
                                .iter()
                                .rev()
                                .find(|preset| preset.effort != ReasoningEffort::Ultra)
                        })
                        .map(|preset| preset.effort.clone())
                })
                .unwrap_or(ReasoningEffort::Medium),
            // Keep "persistent" in local settings; the Responses API calls it "disabled".
            ReasoningEffort::Persistent => ReasoningEffort::Custom("disabled".to_string()),
            effort => effort,
        }
    }
}
#[cfg(test)]
#[path = "reasoning_effort_tests.rs"]
mod tests;
