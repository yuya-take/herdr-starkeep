# Starkeep

herdrで動いているエージェントを、軌道上の修練船で働く騎士として描くプラグインです。
サブエージェントは見習いとして入ってきて、仕事を終えると成果のキューブを騎士に渡して出ていきます。

- 騎士の状態（working / blocked / done / idle / unknown）は、herdrのソケットから0.5秒ごとに取得します
- 見習いの出入りは、Claude Codeのフックが書き出す `~/.local/state/starkeep/events.jsonl` から取得します
- 騎士が3人以下のときは寄りの画面、4人以上のときはフロアを積み重ねた全体図で表示します

## 必要なもの

- herdr 0.9.0 以降
- Rust 1.84 以降（`cargo`）
- `jq`（フックで使います）

## インストール

```sh
cargo build --release
herdr plugin link "$PWD"
herdr plugin pane open --plugin starkeep --entrypoint ship
```

GitHubに置いた場合は、`herdr plugin install <owner>/<repo>` でもインストールできます（ビルドはherdrが行います）。

画面全体の90%のポップアップで開きます。キーに割り当てる場合は、herdrの `config.toml` に次を追加して `herdr server reload-config` を実行します（`prefix+k` は標準でペイン移動に使われています）。

```toml
[[keys.command]]
key = "prefix+shift+k"
type = "plugin_action"
command = "starkeep.open"
description = "open starkeep"
```

### 見習い（サブエージェント）を表示する

Claude Codeのフックを設定します。

```sh
hooks/install.sh
```

このスクリプトは、`hooks/hook.sh` を `~/.config/starkeep/hook.sh` にリンクし、`hooks/settings.json` のフック設定を `~/.claude/settings.json` に追加します。元の設定ファイルは `settings.json.starkeep-backup` として残ります。

| フック | 記録するイベント |
| --- | --- |
| `PreToolUse`（Agent / Task） | 見習いが呼ばれた。タスクの説明を記録 |
| `SubagentStart` | 見習いのIDを記録 |
| `SubagentStop` | 見習いが報告に来る |
| `SessionEnd` | そのセッションの見習いを全員帰す |

フックは `HERDR_PANE_ID` で騎士（ペイン）と結び付けます。herdrの外で動いているClaude Codeでは何も書き出しません。

## 操作

| キー | 全体図 | 寄りの画面 |
| --- | --- | --- |
| ← → ↑ ↓ / hjkl | 騎士を選ぶ | 隣の騎士へずらす |
| Enter / クリック | 選んだ騎士に寄る | その騎士のペインへ移動して閉じる（Enter） |
| Esc | 閉じる | 全体図に戻る（クリックでも戻る） |
| q | 終了 | 終了 |

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
src/model.rs        騎士、見習い、フロアの状態
src/spots.rs        見習いの居場所の割り当てと繰り上がり
src/view.rs         全体図と寄りの切り替え、描画
src/anim.rs         歩行、一礼、星の流れ
src/sprites.rs      文字列とパレットのドット絵
src/render.rs       ハーフブロックのフレームバッファと文字の層
src/demo.rs         --demo 用の模擬データ
hooks/hook.sh       Claude Code から呼ぶスクリプト
```
