#!/bin/sh
# Install the Starkeep hook for Claude Code:
#   1. copy hooks/hook.sh to ~/.config/starkeep/hook.sh
#   2. merge hooks/settings.json into ~/.claude/settings.json (backup kept)
# Also run by the plugin action `starkeep.setup-hooks`.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
notify() {
    echo "$1"
    [ -n "${HERDR_BIN_PATH:-}" ] && "$HERDR_BIN_PATH" notification show Starkeep --body "$1" >/dev/null 2>&1 || true
}
if ! command -v jq >/dev/null 2>&1; then
    notify "hooks not installed: jq is required"
    exit 1
fi

# A copy, not a link, so reinstalling the plugin never changes the hook
# behind Claude Code's back. Remove first: an older install left a symlink.
mkdir -p "$HOME/.config/starkeep"
rm -f "$HOME/.config/starkeep/hook.sh"
cp "$here/hook.sh" "$HOME/.config/starkeep/hook.sh"
chmod 755 "$HOME/.config/starkeep/hook.sh"

settings="$HOME/.claude/settings.json"
mkdir -p "$(dirname "$settings")"
[ -f "$settings" ] || echo '{}' > "$settings"
cp "$settings" "$settings.starkeep-backup"
# Append our matcher groups to each event, skipping ones already installed.
jq --slurpfile add "$here/settings.json" '
  reduce ($add[0].hooks | to_entries[]) as $e (.;
    .hooks[$e.key] = ((.hooks[$e.key] // []) as $cur
      | $cur + [$e.value[] | select(. as $g | $cur | index([$g]) | not)]))
' "$settings.starkeep-backup" > "$settings"
notify "Claude Code hooks installed. Restart running Claude sessions to see apprentices."
