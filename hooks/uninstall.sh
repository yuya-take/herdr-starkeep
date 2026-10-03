#!/bin/sh
# Remove the Starkeep hook from ~/.claude/settings.json (backup kept) and
# delete ~/.config/starkeep/hook.sh. Run by `starkeep.remove-hooks`.
set -eu
notify() {
    echo "$1"
    [ -n "${HERDR_BIN_PATH:-}" ] && "$HERDR_BIN_PATH" notification show Starkeep --body "$1" >/dev/null 2>&1 || true
}
settings="$HOME/.claude/settings.json"
if [ -f "$settings" ]; then
    if ! command -v jq >/dev/null 2>&1; then
        notify "hooks not removed: jq is required"
        exit 1
    fi
    cp "$settings" "$settings.starkeep-backup"
    # Drop every matcher group that runs our hook, then any event left empty.
    jq '
      if .hooks then
        .hooks |= (with_entries(.value |= map(select(
                     any(.hooks[]?; .command == "~/.config/starkeep/hook.sh") | not)))
                   | with_entries(select(.value | length > 0)))
      else . end
    ' "$settings.starkeep-backup" > "$settings"
fi
rm -f "$HOME/.config/starkeep/hook.sh"
notify "Claude Code hooks removed."
