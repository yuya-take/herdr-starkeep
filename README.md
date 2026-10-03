# Starkeep

herdrで動いているエージェントを、軌道上の修練船で働く騎士として描くプラグインです。
サブエージェントは見習いとして入ってきて、仕事を終えると成果のキューブを騎士に渡して出ていきます。

- 騎士の状態（working / blocked / done / idle / unknown）は、herdrのソケットから0.5秒ごとに取得します
- 見習いの出入りは、Claude Codeのフックが書き出す `~/.local/state/starkeep/events.jsonl` から取得します
- herdrのSpace（ワークスペース）1つを1部屋として、Spaceの数に合わせて 1×1・1×2・1×3・2×2・2×3 に並べます。7つ以上は横3つずつの行を縦にスクロールします
- 1部屋にデスクは騎士の数だけ最大4つ。5人以上いるSpaceは見出しに「+N more」を出します（隠れた騎士が応答待ちなら琥珀色）
- エージェントのいないSpaceは「不在」の部屋として表示します
- 部屋を選んで Enter を押すと、そのSpaceの寄りの画面になります

## 必要なもの

- herdr 0.9.0 以降
- `jq`（Claude Code のフックで使います）
- macOS（Apple Silicon / Intel）と Linux（x86_64 / arm64）はビルド済みの実行ファイルを使います。それ以外の環境では Rust（`cargo`）でビルドします

## インストール

```sh
herdr plugin install yuya-take/herdr-starkeep
herdr plugin action invoke starkeep.setup-hooks   # 見習い（サブエージェント）を表示する
herdr plugin action invoke starkeep.setup-key     # prefix+shift+k で開く
```

1行目で、herdrがリポジトリを取得し、GitHub Releases からこの環境向けの実行ファイルをダウンロードします。ダウンロードしたファイルは SHA-256 のチェックサムで確認し、一致しなければインストールを中止します。

開くと画面の90%のポップアップになります。キーを割り当てずに開くときは `herdr plugin action invoke starkeep.open` です。

### キーの割り当て

`starkeep.setup-key` は、herdrの `config.toml` に次の設定を追加して、設定を再読み込みします。元のファイルは `config.toml.starkeep-backup` として残ります。別のキーにしたいときは、アクションを使わずに次の設定を手で書き、`key` を変えてから `herdr server reload-config` を実行します（`prefix+k` は標準でペイン移動に使われています）。

```toml
[[keys.command]]
key = "prefix+shift+k"
type = "plugin_action"
command = "starkeep.open"
description = "open starkeep"
```

`herdr --remote` でつないでいる場合、独自コマンドのキーは手元の設定では効きません。サーバー側でこのアクションを実行し、`herdr --remote <接続先> --remote-keybindings server` でつないでください。

### 見習い（サブエージェント）の表示

`starkeep.setup-hooks` は、`hooks/hook.sh` を `~/.config/starkeep/hook.sh` にコピーし、`hooks/settings.json` のフック設定を `~/.claude/settings.json` に追加します。元の設定ファイルは `settings.json.starkeep-backup` として残ります。設定後に起動した Claude Code から有効になります。外すときは `starkeep.remove-hooks` です。

| フック | 記録するイベント |
| --- | --- |
| `PreToolUse`（Agent / Task） | 見習いが呼ばれた。タスクの説明を記録 |
| `SubagentStart` | 見習いのIDを記録 |
| `SubagentStop` | 見習いが報告に来る |
| `SessionEnd` | そのセッションの見習いを全員帰す |

フックは `HERDR_PANE_ID` で騎士（ペイン）と結び付けます。herdrの外で動いているClaude Codeでは何も書き出しません。

## 操作

| キー | 部屋の一覧 | 寄りの画面 |
| --- | --- | --- |
| ← → ↑ ↓ / hjkl | 部屋（Space）を選ぶ | 隣の騎士へずらす |
| Enter / クリック | 選んだ部屋に寄る（応答待ちの騎士から） | その騎士のペインへ移動して閉じる（Enter） |
| Esc | 閉じる | 全体図に戻る（クリックでも戻る） |
| q | 終了 | 終了 |

## 開発

手元の作業フォルダをそのままherdrにつなぎます。

```sh
cargo build --release --locked
herdr plugin link "$PWD"
herdr plugin pane open --plugin starkeep --entrypoint ship
```

### リリース

`herdr-plugin.toml` と `Cargo.toml` の `version` を上げてマージし、同じ番号のタグ（例: `v0.2.0`）をpushします。CIが各環境向けにビルドして GitHub Releases に置きます。

## デモ

herdrやフックがなくても動きを確認できます。

```sh
cargo run --release -- --demo           # 7人
cargo run --release -- --knights 15     # 15人
```

## 構成

```
herdr-plugin.toml   ペイン「ship」と、それを開くアクション「open」
src/main.rs         描画ループ、キー入力、イベント受信
src/herdr.rs        herdrソケット（agent.list / workspace.list）
src/hooks.rs        events.jsonl の読み込みと追従
src/model.rs        騎士、見習い、部屋（Space）の状態
src/spots.rs        見習いの居場所の割り当てと繰り上がり
src/view.rs         部屋の一覧と寄りの切り替え、描画
src/anim.rs         歩行、一礼、星の流れ
src/sprites.rs      文字列とパレットのドット絵
src/render.rs       ハーフブロックのフレームバッファと文字の層
src/demo.rs         --demo 用の模擬データ
hooks/hook.sh       Claude Code から呼ぶスクリプト
hooks/install.sh    フックの設定（starkeep.setup-hooks）
hooks/uninstall.sh  フックの削除（starkeep.remove-hooks）
scripts/install-binary.sh  インストール時の実行ファイル取得（なければビルド）
scripts/setup-key.sh       キーの割り当て（starkeep.setup-key）
```
