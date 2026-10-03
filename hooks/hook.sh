#!/bin/sh
# Claude Code hook for Starkeep. Appends one JSON line per subagent event.
# Stdin: the hook input Claude Code passes (JSON). Never blocks Claude.
umask 077  # events file and dir: owner-only
[ -n "${HERDR_PANE_ID:-}" ] || exit 0
command -v jq >/dev/null 2>&1 || exit 0
out="${STARKEEP_EVENTS:-${XDG_STATE_HOME:-$HOME/.local/state}/starkeep/events.jsonl}"
mkdir -p "$(dirname "$out")" 2>/dev/null
jq -c --arg pane "$HERDR_PANE_ID" '
  ({PreToolUse: "task", SubagentStart: "start", SubagentStop: "stop", SessionEnd: "end"}[.hook_event_name]) as $ev
  | select($ev != null)
  | {ev: $ev, pane: $pane, session: .session_id,
     agent_id: (.agent_id // null),
     agent_type: (.agent_type // .tool_input.subagent_type // null),
     task: (.tool_input.description // null),
     ts: now}' >> "$out" 2>/dev/null
exit 0
