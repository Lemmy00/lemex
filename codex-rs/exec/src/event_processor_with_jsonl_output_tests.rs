use super::*;
use pretty_assertions::assert_eq;
use serde_json::json;
use tempfile::tempdir;

fn started(item: ThreadItem) -> ServerNotification {
    ServerNotification::ItemStarted(codex_app_server_protocol::ItemStartedNotification {
        item,
        thread_id: "thread-1".to_string(),
        turn_id: "turn-1".to_string(),
        started_at_ms: 0,
    })
}

fn completed(item: ThreadItem) -> ServerNotification {
    ServerNotification::ItemCompleted(codex_app_server_protocol::ItemCompletedNotification {
        item,
        thread_id: "thread-1".to_string(),
        turn_id: "turn-1".to_string(),
        completed_at_ms: 0,
    })
}

fn summary_delta(id: &str, index: i64, delta: &str) -> ServerNotification {
    ServerNotification::ReasoningSummaryTextDelta(
        codex_app_server_protocol::ReasoningSummaryTextDeltaNotification {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            item_id: id.to_string(),
            summary_index: index,
            delta: delta.to_string(),
        },
    )
}

fn trace_delta(id: &str, index: i64, delta: &str) -> ServerNotification {
    ServerNotification::ReasoningTextDelta(
        codex_app_server_protocol::ReasoningTextDeltaNotification {
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            item_id: id.to_string(),
            content_index: index,
            delta: delta.to_string(),
        },
    )
}

fn serialized(events: CollectedThreadEvents) -> Vec<serde_json::Value> {
    assert_eq!(events.status, CodexStatus::Running);
    events
        .events
        .into_iter()
        .map(|event| serde_json::to_value(event).expect("serialize event"))
        .collect()
}

#[test]
fn reasoning_visibility_preserves_summary_and_requires_raw_opt_in() {
    for (hide, raw) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut processor = EventProcessorWithJsonOutput::new(None);
        processor.set_reasoning_visibility(hide, raw);
        let beginning = serialized(processor.collect_thread_events(started(
            ThreadItem::Reasoning {
                id: "reasoning-1".to_string(),
                summary: Vec::new(),
                content: Vec::new(),
            },
        )));
        let summary =
            serialized(processor.collect_thread_events(summary_delta("reasoning-1", 0, "Summary")));
        let trace =
            serialized(processor.collect_thread_events(trace_delta("reasoning-1", 0, "Trace")));
        let ending = serialized(processor.collect_thread_events(completed(
            ThreadItem::Reasoning {
                id: "reasoning-1".to_string(),
                summary: vec!["Summary".to_string()],
                content: vec!["Trace".to_string()],
            },
        )));
        if hide {
            assert_eq!(
                (beginning, summary, trace, ending),
                (vec![], vec![], vec![], vec![])
            );
            continue;
        }
        assert_eq!(
            beginning,
            vec![
                json!({"type":"item.started","item":{"id":"item_0","type":"reasoning","text":""}})
            ]
        );
        assert_eq!(
            summary,
            vec![
                json!({"type":"item.delta","item_id":"item_0","item_type":"reasoning","field":"text","delta":"Summary","reasoning_kind":"summary"})
            ]
        );
        if raw {
            assert_eq!(
                trace,
                vec![
                    json!({"type":"item.delta","item_id":"item_0","item_type":"reasoning","field":"raw_text","delta":"Trace","reasoning_kind":"trace"})
                ]
            );
            assert_eq!(
                ending,
                vec![
                    json!({"type":"item.completed","item":{"id":"item_0","type":"reasoning","text":"Summary","raw_text":"Trace"}})
                ]
            );
        } else {
            assert!(trace.is_empty());
            assert_eq!(
                ending,
                vec![
                    json!({"type":"item.completed","item":{"id":"item_0","type":"reasoning","text":"Summary"}})
                ]
            );
        }
    }
}

#[test]
fn raw_only_reasoning_is_completed_when_exposed() {
    let mut processor = EventProcessorWithJsonOutput::new(None);
    processor.set_reasoning_visibility(false, true);
    let delta =
        serialized(processor.collect_thread_events(trace_delta("raw-only", 0, "Provider trace")));
    let ending = serialized(
        processor.collect_thread_events(completed(ThreadItem::Reasoning {
            id: "raw-only".to_string(),
            summary: Vec::new(),
            content: vec!["Provider trace".to_string()],
        })),
    );
    assert_eq!(delta[0]["item_id"], "item_0");
    assert_eq!(
        ending,
        vec![
            json!({"type":"item.completed","item":{"id":"item_0","type":"reasoning","text":"","raw_text":"Provider trace"}})
        ]
    );
}

#[test]
fn multipart_reasoning_deltas_join_to_completed_fields() {
    let mut processor = EventProcessorWithJsonOutput::new(None);
    processor.set_reasoning_visibility(false, true);
    let notifications = [
        summary_delta("r", 0, "First"),
        trace_delta("r", 0, "Raw"),
        summary_delta("r", 1, "Sec"),
        summary_delta("r", 1, "ond"),
        trace_delta("r", 2, "Last"),
    ];
    let mut summary = String::new();
    let mut trace = String::new();
    for notification in notifications {
        let events = serialized(processor.collect_thread_events(notification));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["item_id"], "item_0");
        let text = events[0]["delta"].as_str().expect("delta is text");
        match events[0]["field"].as_str() {
            Some("text") => summary.push_str(text),
            Some("raw_text") => trace.push_str(text),
            field => panic!("unexpected delta field: {field:?}"),
        }
    }
    let ending = serialized(
        processor.collect_thread_events(completed(ThreadItem::Reasoning {
            id: "r".to_string(),
            summary: vec!["First".to_string(), "Second".to_string()],
            content: vec!["Raw".to_string(), String::new(), "Last".to_string()],
        })),
    );
    assert_eq!(summary, ending[0]["item"]["text"]);
    assert_eq!(trace, ending[0]["item"]["raw_text"]);
    assert_eq!(ending[0]["item"]["id"], "item_0");
}

#[test]
fn agent_message_deltas_share_start_and_completion_id() {
    let mut processor = EventProcessorWithJsonOutput::new(None);
    let item = |text: &str| ThreadItem::AgentMessage {
        id: "message-1".to_string(),
        text: text.to_string(),
        phase: None,
        memory_citation: None,
        delivery: None,
        questions: None,
    };
    let beginning = serialized(processor.collect_thread_events(started(item(""))));
    let mut output = String::new();
    for delta in ["Checking ", "Lean."] {
        let events = serialized(processor.collect_thread_events(
            ServerNotification::AgentMessageDelta(
                codex_app_server_protocol::AgentMessageDeltaNotification {
                    thread_id: "thread-1".to_string(),
                    turn_id: "turn-1".to_string(),
                    item_id: "message-1".to_string(),
                    delta: delta.to_string(),
                },
            ),
        ));
        assert_eq!(
            events,
            vec![
                json!({"type":"item.delta","item_id":"item_0","item_type":"agent_message","field":"text","delta":delta})
            ]
        );
        output.push_str(delta);
    }
    let ending = serialized(processor.collect_thread_events(completed(item(&output))));
    assert_eq!(beginning[0]["item"]["id"], ending[0]["item"]["id"]);
    assert_eq!(ending[0]["item"]["text"], output);
    assert_eq!(processor.final_message(), Some("Checking Lean."));
}

#[test]
fn empty_deltas_do_not_reserve_ids() {
    let mut processor = EventProcessorWithJsonOutput::new(None);
    assert!(
        processor
            .collect_thread_events(summary_delta("empty", 0, ""))
            .events
            .is_empty()
    );
    assert!(
        processor
            .collect_thread_events(trace_delta("hidden", 0, "private"))
            .events
            .is_empty()
    );
    let ending = serialized(
        processor.collect_thread_events(completed(ThreadItem::Reasoning {
            id: "summary".to_string(),
            summary: vec!["Visible".to_string()],
            content: Vec::new(),
        })),
    );
    assert_eq!(ending[0]["item"]["id"], "item_0");
}

#[test]
fn started_reasoning_is_closed_without_exposing_hidden_raw_content() {
    let mut processor = EventProcessorWithJsonOutput::new(None);
    let beginning = serialized(
        processor.collect_thread_events(started(ThreadItem::Reasoning {
            id: "empty-summary".to_string(),
            summary: Vec::new(),
            content: Vec::new(),
        })),
    );
    let ending = serialized(
        processor.collect_thread_events(completed(ThreadItem::Reasoning {
            id: "empty-summary".to_string(),
            summary: Vec::new(),
            content: vec!["Not exposed".to_string()],
        })),
    );
    assert_eq!(beginning[0]["item"]["id"], "item_0");
    assert_eq!(
        ending,
        vec![json!({"type":"item.completed","item":{"id":"item_0","type":"reasoning","text":""}})]
    );
    assert!(processor.raw_to_exec_item_id.is_empty());
}

#[test]
fn legacy_reasoning_json_still_deserializes_without_raw_text() {
    let event: ThreadEvent = serde_json::from_value(
        json!({"type":"item.completed","item":{"id":"item_9","type":"reasoning","text":"Summary"}}),
    )
    .expect("legacy event");
    assert_eq!(
        serde_json::to_value(event).expect("serialize"),
        json!({"type":"item.completed","item":{"id":"item_9","type":"reasoning","text":"Summary"}})
    );
}

#[test]
fn failed_turn_does_not_overwrite_output_last_message_file() {
    let tempdir = tempdir().expect("create tempdir");
    let output_path = tempdir.path().join("last-message.txt");
    std::fs::write(&output_path, "keep existing contents").expect("seed output file");

    let mut processor = EventProcessorWithJsonOutput::new(Some(output_path.clone()));

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        codex_app_server_protocol::ItemCompletedNotification {
            item: ThreadItem::AgentMessage {
                id: "msg-1".to_string(),
                text: "partial answer".to_string(),
                phase: None,
                memory_citation: None,
                delivery: None,
                questions: None,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            completed_at_ms: 0,
        },
    ));

    assert_eq!(collected.status, CodexStatus::Running);
    assert_eq!(processor.final_message(), Some("partial answer"));

    let status = processor.process_server_notification(ServerNotification::TurnCompleted(
        codex_app_server_protocol::TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: codex_app_server_protocol::Turn {
                id: "turn-1".to_string(),
                root_turn_id: None,
                items_view: codex_app_server_protocol::TurnItemsView::Full,
                items: Vec::new(),
                status: TurnStatus::Failed,
                error: Some(codex_app_server_protocol::TurnError {
                    misalignment: None,
                    message: "turn failed".to_string(),
                    additional_details: None,
                    codex_error_info: None,
                }),
                started_at: None,
                completed_at: Some(0),
                duration_ms: None,
            },
        },
    ));

    assert_eq!(status, CodexStatus::InitiateShutdown);
    assert_eq!(processor.final_message(), None);

    EventProcessor::print_final_output(&mut processor);

    assert_eq!(
        std::fs::read_to_string(&output_path).expect("read output file"),
        "keep existing contents"
    );
}

#[test]
fn runtime_warning_emits_a_non_fatal_error_item() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_thread_events(ServerNotification::Warning(
        codex_app_server_protocol::WarningNotification {
            thread_id: Some("thread-1".to_string()),
            message: "invalid global instructions".to_string(),
        },
    ));

    assert_eq!(
        collected,
        CollectedThreadEvents {
            events: vec![ThreadEvent::ItemCompleted(ItemCompletedEvent {
                item: ExecThreadItem {
                    id: "item_0".to_string(),
                    details: ThreadItemDetails::Error(ErrorItem {
                        message: "invalid global instructions".to_string(),
                    }),
                },
            })],
            status: CodexStatus::Running,
        }
    );
}

#[test]
fn mcp_tool_call_result_preserves_meta_in_jsonl_event() {
    let mut processor = EventProcessorWithJsonOutput::new(/*last_message_path*/ None);

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        codex_app_server_protocol::ItemCompletedNotification {
            item: ThreadItem::McpToolCall {
                id: "mcp-1".to_string(),
                server: "search service".to_string(),
                tool: "web_run".to_string(),
                status: McpToolCallStatus::Completed,
                arguments: json!({"search_query": [{"q": "Lemex CLI documentation"}]}),
                app_context: None,
                mcp_app_resource_uri: None,
                mcp_app_ui: None,
                plugin_id: None,
                read_only_hint: None,
                result: Some(Box::new(codex_app_server_protocol::McpToolCallResult {
                    content: vec![json!({"type": "text", "text": "search result"})],
                    structured_content: None,
                    meta: Some(json!({"raw_messages": [{"ref_id": "turn0search0"}]})),
                })),
                error: None,
                duration_ms: Some(42),
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
            completed_at_ms: 0,
        },
    ));

    assert_eq!(collected.status, CodexStatus::Running);
    assert_eq!(collected.events.len(), 1);

    let ThreadEvent::ItemCompleted(ItemCompletedEvent { item }) = &collected.events[0] else {
        panic!("expected item.completed event");
    };
    let ThreadItemDetails::McpToolCall(item) = &item.details else {
        panic!("expected MCP tool call item");
    };
    let result = item.result.as_ref().expect("expected MCP tool result");
    assert_eq!(
        result.meta,
        Some(json!({"raw_messages": [{"ref_id": "turn0search0"}]}))
    );

    let serialized = serde_json::to_value(&collected.events[0]).expect("serialize event");
    assert_eq!(
        serialized["item"]["result"]["_meta"],
        json!({"raw_messages": [{"ref_id": "turn0search0"}]})
    );
    assert!(serialized["item"]["result"].get("meta").is_none());
}
