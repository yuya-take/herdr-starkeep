#!/bin/sh
# Install the Starkeep hook for Claude Code:
#   1. link hooks/hook.sh to ~/.config/starkeep/hook.sh
#   2. merge hooks/settings.json into ~/.claude/settings.json (backup kept)
set -eu
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$HOME/.config/starkeep"
ln -sf "$here/hook.sh" "$HOME/.config/starkeep/hook.sh"

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
echo "installed: $HOME/.config/starkeep/hook.sh"
echo "updated:   $settings (backup: $settings.starkeep-backup)"
