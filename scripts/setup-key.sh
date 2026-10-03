#!/bin/sh
# Bind a herdr key to open Starkeep and reload the config. Run by
# `starkeep.setup-key`. Running the script directly, STARKEEP_KEY picks
# another key (actions run in the herdr server, so they don't see it).
set -eu
key="${STARKEEP_KEY:-prefix+shift+k}"
herdr="${HERDR_BIN_PATH:-herdr}"
config="${HERDR_CONFIG_PATH:-$HOME/.config/herdr/config.toml}"
notify() {
    echo "$1"
    "$herdr" notification show Starkeep --body "$1" >/dev/null 2>&1 || true
}
mkdir -p "$(dirname "$config")"
touch "$config"
if grep -q '"starkeep.open"' "$config"; then
    notify "a key for Starkeep is already in $config"
    exit 0
fi
if grep -q "\"$key\"" "$config"; then
    notify "$key is already bound in $config; set STARKEEP_KEY to pick another key"
    exit 1
fi
cp "$config" "$config.starkeep-backup"
cat >> "$config" <<TOML

# Starkeep: show every agent as a knight; enter goes to the pane, esc closes.
[[keys.command]]
key = "$key"
type = "plugin_action"
command = "starkeep.open"
description = "open starkeep"
TOML
"$herdr" server reload-config >/dev/null 2>&1 || true
notify "Starkeep is on $key"
