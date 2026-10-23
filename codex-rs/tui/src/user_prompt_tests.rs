use super::extract_prompt_request_with_offset;
use pretty_assertions::assert_eq;

#[test]
fn extract_prompt_request_returns_text_after_last_delimiter() {
    let message =
        "# Context\n## My request for Lemex:\nFirst\n## My request for Lemex:\n  Second\n";

    assert_eq!(
        extract_prompt_request_with_offset(message),
        ("Second", message.find("Second").expect("request offset"))
    );
}

#[test]
fn mixed_legacy_and_lemex_delimiters_preserve_the_last_request_offset() {
    for message in [
        "## My request for Codex:\nOld\n## My request for Lemex:\n  Latest 🦀\n",
        "## My request for Lemex:\nOld\n## My request for Codex:\n  Latest 🦀\n",
    ] {
        assert_eq!(
            extract_prompt_request_with_offset(message),
            ("Latest 🦀", message.find("Latest").unwrap())
        );
    }
}
