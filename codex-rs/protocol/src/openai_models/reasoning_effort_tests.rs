use super::super::ReasoningEffortPreset;
use super::super::tests::test_model;
use super::ReasoningEffort;
use pretty_assertions::assert_eq;

#[test]
fn model_defaults_prefer_supported_reasoning() {
    use ReasoningEffort::High;
    use ReasoningEffort::Low;
    use ReasoningEffort::Max;
    use ReasoningEffort::Medium;
    use ReasoningEffort::None as Off;
    use ReasoningEffort::Ultra;

    let disabled = ReasoningEffort::Custom("disabled".to_string());
    for (declared, supported, expected) in [
        (Some(Max), vec![Off, Medium, Max], Some(Max)),
        (None, vec![Off, Low, Medium, High], Some(Medium)),
        (Some(Off), vec![Off, High], Some(High)),
        (Some(disabled.clone()), vec![disabled, Low], Some(Low)),
        (Some(Max), vec![Low, High], Some(Low)),
        (Some(Off), vec![Off, Ultra], Some(Off)),
        (None, vec![], None),
        (Some(High), vec![], Some(High)),
    ] {
        let mut model = test_model(/*spec*/ None);
        model.default_reasoning_level = declared;
        model.supported_reasoning_levels = supported
            .into_iter()
            .map(|effort| ReasoningEffortPreset {
                effort,
                description: String::new(),
            })
            .collect();
        assert_eq!(model.preferred_reasoning_effort(), expected.as_ref());
        let preset = super::super::ModelPreset::from(model);
        assert_eq!(preset.default_reasoning_effort, expected.unwrap_or(Off));
    }
}

#[test]
fn resolves_ultra_using_model_override_and_supported_fallbacks() {
    use ReasoningEffort::High;
    use ReasoningEffort::Low;
    use ReasoningEffort::Max;
    use ReasoningEffort::Medium;
    use ReasoningEffort::Ultra;
    use ReasoningEffort::XHigh;

    for (supported, multi_agent_effort, expected) in [
        (vec![High, Max, Ultra], Some(High), High),
        (vec![High, Max, Ultra], None, Max),
        (vec![High, XHigh, Ultra], None, XHigh),
        (vec![High, XHigh, Ultra], Some(Ultra), XHigh),
        (vec![High, XHigh, Ultra], Some(Low), XHigh),
        (vec![], None, Medium),
    ] {
        let mut model = test_model(/*spec*/ None);
        model.supported_reasoning_levels = supported
            .into_iter()
            .map(|effort| ReasoningEffortPreset {
                effort,
                description: String::new(),
            })
            .collect();
        model.multi_agent_reasoning_effort = multi_agent_effort;
        assert_eq!(model.resolve_reasoning_effort(Ultra), expected);
    }
}

#[test]
fn preserves_native_efforts_and_translates_persistent_mode() {
    let model = test_model(/*spec*/ None);
    for effort in [
        ReasoningEffort::Low,
        ReasoningEffort::Medium,
        ReasoningEffort::High,
        ReasoningEffort::Max,
        ReasoningEffort::Custom("future-effort".to_string()),
    ] {
        assert_eq!(model.resolve_reasoning_effort(effort.clone()), effort);
    }
    assert_eq!(
        model.resolve_reasoning_effort(ReasoningEffort::Persistent),
        ReasoningEffort::Custom("disabled".to_string()),
    );
}
