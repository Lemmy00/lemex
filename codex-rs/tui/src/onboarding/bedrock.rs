//! Guidance for configured Amazon Bedrock GovCloud endpoints.

use super::keys;
use crate::key_hint::KeyBindingListExt;
use crate::wrapping::word_wrap_lines;
use codex_app_server_client::AppServerRequestHandle;
use codex_app_server_protocol::BedrockCheckGovCloudRequirementsParams;
use codex_app_server_protocol::BedrockCheckGovCloudRequirementsResponse;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::RequestId;
use crossterm::event::KeyEvent;
use crossterm::event::KeyEventKind;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::prelude::Widget;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

const GOV_CLOUD_GUIDANCE_URL: &str =
    "https://learn.chatgpt.com/docs/enterprise/govcloud-configuration";

pub(super) struct BedrockState {
    scroll: AtomicUsize,
}

impl BedrockState {
    pub(super) fn guidance() -> Self {
        Self {
            scroll: AtomicUsize::new(0),
        }
    }

    pub(super) fn handle_guidance_key_event(&mut self, key_event: &KeyEvent) -> bool {
        let offset = self.scroll.load(Ordering::Relaxed);
        if keys::MOVE_UP.is_pressed(*key_event) {
            self.scroll
                .store(offset.saturating_sub(1), Ordering::Relaxed);
        } else if keys::MOVE_DOWN.is_pressed(*key_event) {
            self.scroll
                .store(offset.saturating_add(1), Ordering::Relaxed);
        }
        key_event.kind == KeyEventKind::Press && keys::CONFIRM.is_pressed(*key_event)
    }

    pub(super) fn render(&self, area: Rect, buf: &mut Buffer, error: Option<String>) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let mut lines: Vec<Line> = vec!["Using Lemex with AWS GovCloud".bold().into(), "".into()];
        lines.push(Line::from(vec![
            "You are using Lemex with AWS GovCloud. Please ensure you or your administrator have read the ".into(),
            "application configuration and security guidance".cyan().underlined(),
            " before proceeding.".into(),
        ]));
        let mut footer = vec![
            Line::default(),
            "> Acknowledge".cyan().into(),
            Line::default(),
            Line::from(vec![
                "  Press ".dim(),
                keys::CONFIRM[0].into(),
                " to continue".dim(),
            ]),
        ];
        if let Some(error) = error {
            footer.push("".into());
            footer.push(error.red().into());
        }
        let mut lines = word_wrap_lines(lines, usize::from(area.width));
        let mut footer = word_wrap_lines(footer, usize::from(area.width));
        if lines.len() + footer.len() > usize::from(area.height) {
            footer = vec!["> Acknowledge".cyan().into()];
            if area.height > 2 {
                footer.push(Line::from(vec![
                    keys::MOVE_UP[0].into(),
                    "/".dim(),
                    keys::MOVE_DOWN[0].into(),
                    " scroll · ".dim(),
                    keys::CONFIRM[0].into(),
                ]));
            }
            footer = word_wrap_lines(footer, usize::from(area.width));
        }
        if lines.len() + footer.len() <= usize::from(area.height) || footer.is_empty() {
            lines.extend(footer);
            Paragraph::new(lines).render(area, buf);
            self.scroll.store(0, Ordering::Relaxed);
            crate::terminal_hyperlinks::mark_underlined_hyperlink(
                buf,
                area,
                GOV_CLOUD_GUIDANCE_URL,
            );
            return;
        }
        let footer_height = u16::try_from(footer.len())
            .unwrap_or(u16::MAX)
            .min(area.height.saturating_sub(1));
        let content_height = area.height.saturating_sub(footer_height);
        let max_scroll = lines.len().saturating_sub(usize::from(content_height));
        let scroll = self.scroll.load(Ordering::Relaxed).min(max_scroll);
        self.scroll.store(scroll, Ordering::Relaxed);
        let content_area = Rect {
            height: content_height,
            ..area
        };
        let footer_area = Rect {
            y: area.y.saturating_add(content_height),
            height: footer_height,
            ..area
        };
        Paragraph::new(lines)
            .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0))
            .render(content_area, buf);
        Paragraph::new(footer).render(footer_area, buf);
        crate::terminal_hyperlinks::mark_underlined_hyperlink(
            buf,
            content_area,
            GOV_CLOUD_GUIDANCE_URL,
        );
    }
}

pub(crate) async fn check_gov_cloud(request_handle: AppServerRequestHandle) -> bool {
    match tokio::time::timeout(
        std::time::Duration::from_secs(/*secs*/ 15),
        request_handle.request_typed::<BedrockCheckGovCloudRequirementsResponse>(
            ClientRequest::BedrockCheckGovCloudRequirements {
                request_id: RequestId::String(format!("tui-gov-cloud-{}", uuid::Uuid::new_v4())),
                params: BedrockCheckGovCloudRequirementsParams {},
            },
        ),
    )
    .await
    {
        Ok(Ok(response)) => response.is_gov_cloud,
        Ok(Err(_)) | Err(_) => false,
    }
}

#[cfg(test)]
#[path = "bedrock_tests.rs"]
mod tests;
