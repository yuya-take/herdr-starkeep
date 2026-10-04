# Starkeep

A herdr plugin that draws the agents running in herdr as knights working on an orbital training ship.
Subagents come in as apprentices, work beside their knight, and leave after handing over a glowing cube with their results.

- Knight state (working / blocked / done / idle / unknown) is read from the herdr socket every 0.5 seconds
- Apprentices come and go based on `~/.local/state/starkeep/events.jsonl`, written by a Claude Code hook
- Each herdr Space (workspace) is one room. Rooms are laid out 1×1, 1×2, 1×3, 2×2 or 2×3 depending on how many Spaces there are; with seven or more, rows of three scroll vertically
- A room has one desk per knight, up to four. A Space with five or more shows "+N more" in its header, in amber when a hidden knight is waiting for an answer
- Each desk is lit by its knight's state: bright while working, dark at rest, green when done, and blinking amber while waiting for an answer
- Spaces without agents show as empty rooms
- Select a room and press Enter to zoom into that Space

## Requirements

- herdr 0.9.0 or later
- `jq` (used by the Claude Code hook)
- Prebuilt binaries are used on macOS (Apple Silicon / Intel) and Linux (x86_64 / arm64). Elsewhere Starkeep is built with Rust (`cargo`)

## Install

```sh
herdr plugin install yuya-take/herdr-starkeep
herdr plugin action invoke starkeep.setup-hooks   # show apprentices (subagents)
herdr plugin action invoke starkeep.setup-key     # open with prefix+shift+k
```

The first command makes herdr fetch the repository and download the binary for your platform from GitHub Releases. The download is checked against its SHA-256 checksum, and the install stops if they don't match.

Starkeep opens as a popup covering 90% of the screen. To open it without a key binding, run `herdr plugin action invoke starkeep.open`.

Actions run in the background. Check their results with:

```sh
herdr plugin log list --plugin starkeep
```

### Key binding

`starkeep.setup-key` adds the following to herdr's `config.toml` and reloads the config. The previous file is kept as `config.toml.starkeep-backup`. For a different key, skip the action, add this block by hand with another `key`, and run `herdr server reload-config` (`prefix+k` is taken by default for moving between panes).

```toml
[[keys.command]]
key = "prefix+shift+k"
type = "plugin_action"
command = "starkeep.open"
description = "open starkeep"
```

With `herdr --remote`, custom command key bindings from the local config are ignored. Install Starkeep and run the actions on the server, then connect with `herdr --remote <target> --remote-keybindings server`.

### Apprentices (subagents)

`starkeep.setup-hooks` copies `hooks/hook.sh` to `~/.config/starkeep/hook.sh` and adds the hooks in `hooks/settings.json` to `~/.claude/settings.json`. The previous settings file is kept as `settings.json.starkeep-backup`. The hooks take effect in Claude Code sessions started afterwards. Remove them with `starkeep.remove-hooks`.

| Hook | Event recorded |
| --- | --- |
| `PreToolUse` (Agent / Task) | An apprentice is summoned; records the task description |
| `SubagentStart` | Records the apprentice's id |
| `SubagentStop` | The apprentice comes to report |
| `SessionEnd` | Sends every apprentice of that session home |

The hook ties apprentices to their knight (pane) with `HERDR_PANE_ID`. It writes nothing for Claude Code running outside herdr.

## Controls

| Key | Rooms | Close-up |
| --- | --- | --- |
| ← → ↑ ↓ / hjkl | Select a room (Space) | Move to the next knight |
| Enter / click | Zoom into the room, starting with a knight waiting for an answer | Go to that knight's pane and close (Enter) |
| Esc | Close | Back to the rooms (click also works) |
| q | Quit | Quit |

## Development

Link your working copy to herdr:

```sh
cargo build --release --locked
herdr plugin link "$PWD"
herdr plugin pane open --plugin starkeep --entrypoint ship
```

### Releasing

Bump `version` in `herdr-plugin.toml` and `Cargo.toml`, merge, then push a tag with the same number (for example `v0.2.0`). CI builds every platform and uploads the binaries to GitHub Releases.

## Demo

Try it without herdr or the hook:

```sh
cargo run --release -- --demo           # 7 knights
cargo run --release -- --knights 15     # 15 knights
```

## Layout

```
herdr-plugin.toml          the "ship" pane and the actions that open and set it up
src/main.rs                render loop, key input, event intake
src/herdr.rs               herdr socket (agent.list / workspace.list)
src/hooks.rs               reading and following events.jsonl
src/model.rs               knights, apprentices and rooms (Spaces)
src/spots.rs               where apprentices stand, and moving them up
src/view.rs                the rooms grid and the close-up, and drawing them
src/anim.rs                walking, bowing, star fields
src/sprites.rs             pixel art as strings and palettes
src/render.rs              half-block frame buffer and text layer
src/demo.rs                simulated data for --demo
hooks/hook.sh              script Claude Code runs
hooks/install.sh           installs the hook (starkeep.setup-hooks)
hooks/uninstall.sh         removes the hook (starkeep.remove-hooks)
scripts/install-binary.sh  fetches the binary at install time, or builds it
scripts/setup-key.sh       binds the key (starkeep.setup-key)
```
