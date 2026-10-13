# Non-interactive mode

For information about non-interactive mode, see [this documentation](https://developers.openai.com/codex/noninteractive).

## Live JSON output

`lemex exec --json` writes one JSON event per line. Message and reasoning items
emit `item.started` and `item.completed`; commands also include their command and
execution status. These events use the same stable item ID throughout an item.

While an item runs, `item.delta` appends a text fragment without resending its full
contents:

```json
{
  "type": "item.delta",
  "item_id": "item_0",
  "item_type": "reasoning",
  "field": "text",
  "delta": "Checking the imports",
  "reasoning_kind": "summary"
}
```

Supported fields are `text` for `agent_message`, `aggregated_output` for
`command_execution`, and `text` or `raw_text` for `reasoning`. Reasoning deltas
identify `reasoning_kind` as `summary` or `trace`. Consumers can append fragments
for a live preview and replace that preview with the authoritative full item on
`item.completed`. Completed records remain available to consumers that ignore
deltas.

Reasoning `text` retains its existing summary meaning. When the native setting
`show_raw_agent_reasoning = true` permits provider-exposed reasoning, the item
also has an optional `raw_text` field, including when no summary is supplied.
Setting `hide_agent_reasoning = true` suppresses all reasoning records and
fragments; setting `show_raw_agent_reasoning = false` suppresses traces while
retaining summaries. The stream does not expose encrypted reasoning or request
additional reasoning from providers.
