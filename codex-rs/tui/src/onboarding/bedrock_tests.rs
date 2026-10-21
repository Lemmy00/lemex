use super::*;
use crossterm::event::KeyCode;
use crossterm::event::KeyModifiers;
use pretty_assertions::assert_eq;

#[test]
fn gov_cloud_guidance_requires_acknowledgement() {
    let mut state = BedrockState::guidance();
    insta::assert_snapshot!("gov_cloud_warning", render_visible(&state));
    assert!(!state.handle_guidance_key_event(&KeyEvent::new_with_kind(
        KeyCode::Enter,
        KeyModifiers::NONE,
        KeyEventKind::Repeat,
    )));
    assert!(state.handle_guidance_key_event(&KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    let area = Rect::new(0, 0, 72, 24);
    let mut buffer = Buffer::empty(area);
    state.render(area, &mut buffer, /*error*/ None);
    assert!(
        buffer
            .content
            .iter()
            .any(|cell| cell.symbol().contains(GOV_CLOUD_GUIDANCE_URL))
    );
}

#[test]
fn gov_cloud_guidance_scrolls_in_short_panes() {
    let mut state = BedrockState::guidance();
    let area = Rect::new(0, 0, 24, 6);
    let initial = render_visible_at(&state, area);
    insta::assert_snapshot!("gov_cloud_warning_short", initial);

    // Even a one-row disclosure viewport must expose every line and the link.
    for height in [6, 2] {
        let area = Rect::new(0, 0, 24, height);
        let mut seen = String::new();
        for _ in 0..20 {
            let mut buffer = Buffer::empty(area);
            state.render(area, &mut buffer, /*error*/ None);
            seen.extend(buffer.content.iter().map(ratatui::buffer::Cell::symbol));
            assert!(render_visible_at(&state, area).contains("Acknowledge"));
            assert!(
                !state.handle_guidance_key_event(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
            );
        }
        assert!(seen.contains("Using Lemex with AWS"));
        assert!(seen.contains("proceeding."));
        assert!(seen.contains(GOV_CLOUD_GUIDANCE_URL));
        for _ in 0..20 {
            state.handle_guidance_key_event(&KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
            render_visible_at(&state, area);
        }
        assert!(render_visible_at(&state, area).starts_with("Using Lemex with AWS"));
    }
    assert_eq!(render_visible_at(&state, area), initial);
    state.handle_guidance_key_event(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    render_visible(&state);
    assert_eq!(render_visible_at(&state, area), initial);
}

fn render_visible(state: &BedrockState) -> String {
    render_visible_at(state, Rect::new(0, 0, 72, 24))
}

fn render_visible_at(state: &BedrockState, area: Rect) -> String {
    let mut buffer = Buffer::empty(area);
    state.render(area, &mut buffer, /*error*/ None);
    let mut rows = (area.top()..area.bottom())
        .map(|row| {
            (area.left()..area.right())
                .map(|column| {
                    crate::terminal_hyperlinks::strip_osc8(buffer[(column, row)].symbol())
                })
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>();
    while rows.last().is_some_and(String::is_empty) {
        rows.pop();
    }
    rows.join("\n")
}
