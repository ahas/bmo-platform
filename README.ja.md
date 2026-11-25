[English](README.md) | [한국어](README.ko.md) | **日本語** | [简体中文](README.zh-CN.md)

# agency

エージェントを供給し、届け、制御するサービスです。この中では二つの軸が動いています。ルームを持ち人と対話する
スーパーバイザーと、作業を実行するロールです。呼び出し側があるパスでロールの実行（run）を求めると、agency は
ロールの上限の範囲でそれを受け入れ、ロールの順序のうちクォータが残っているプロバイダーを割り当てて起動します。
そのうえでターンを運び、エージェントの発言を読み取り、実行を終了させます。herdr はセッション契約の背後にある
バックエンドの一つです。

## ドキュメント

- `docs/api.md`: 境界の契約です。agency が公開・呼び出しするすべてのルートとイベントを扱います。
- `docs/agents.md`: 二つの軸であるスーパーバイザーとロール、実行、そしてすべてのエージェントに到達する唯一の
  インターフェースを説明します。
- `docs/supervisors.md`: スーパーバイザーの行が持つ内容と、一つのランナーがすべてのスーパーバイザーのターンを
  運ぶ仕組みを説明します。
- `docs/agent-roles.md`: ロールの行が持つ内容、ポリシーがシードするロール、そのバンドルとセッションを説明します。
- `docs/repositories.md`: リポジトリの行が持つ内容、そこでの実行に渡されるもの、ジョブと固定されたモデルを
  説明します。
- `docs/job-review.md`: ジョブが終わる前に、発注者がジョブの報告を段階ごとに審査する方法を説明します。
- `docs/provider-order.md`: 実行がプロバイダーとモデルを得る方法です。順序、クォータ、フェイルオーバー、待機を
  扱います。
- `docs/role-slots.md`: ロールの上限の範囲で実行を受け入れる方法です。待ち行列、予約、スーパーバイザーに従う
  ロール、常設のブランチチェックアウトを扱います。
- `docs/sessions.md`: スーパーバイザーのセッションとロールのセッションを保持・再利用・折りたたみ（fold）・
  置き換える方法を説明します。
- `docs/keepalive-schedule.md`: スーパーバイザーのプレフィックスを温めておくために ping を送るタイミングを
  説明します。
- `docs/PERSONAL-DATA.md`: リポジトリに入れない個人データと、それを守るチェックを説明します。
- `docs/` のその他のドキュメント: 実行中のプログラムに言葉を差し込む方法（`agent-control.md`）、起動コマンド
  ライン、アカウントとそのフォールバック、利用量ウィンドウ、登録済みのモデルとモデルファイル、リポジトリの
  チェックアウトとワークベンチ、作業ノード、ツール、実行記録を扱います。

## フォルダー

| Folder | Holds |
|---|---|
| `crates/` | Rust のクレート |
| `deploy/` | サービスユニット |
| `docs/` | 正本となるドキュメント |
| `packages/` | TypeScript パッケージ（一つの Bun ワークスペース） |
| `screen-kit/` | 人の PC で個人アプリの画面をビルドするもの：コンソールと同じリリースの bmo-ui バンドルビルド |
| `policy/` | agency が従う固定値 |
| `prompts/` | agency がエージェントの起動を組み立てるプロンプト |
| `skills/` | リポジトリが同梱するワーカースキル |
| `tests/` | すべてのテスト |
| `tools/` | agency がツールホストに提供するツール |

## 構成

| Path | Holds |
|---|---|
| `crates/agent-session` | セッション契約: 開始、ターン送信、状態の読み取り、中断、折りたたみ、終了、一覧 |
| `crates/herdr` | herdr のソケットクライアント |
| `crates/agent-host` | ペインに置かれた実行の監督役 |
| `crates/pane-head` | ペインの中に立つプロセス |
| `crates/harness-machine` | ハーネスがマシン上で読み書きし、起動するもの |
| `crates/harness-providers` | 各プロバイダーがマシンについて答えるもの |
| `crates/node-link` | マシン間のリンク |
| `crates/llm-gateway` | 登録済みモデルのフロント |
| `crates/policy` | `policy/` のリーダー |
| `crates/cache-siren` | アカウントの利用量ウィンドウ、そのリーダー、キャッシュ寿命の判断 |
| `crates/client` | クレート `bmo-agency-client`: agency API のすべてのボディとメンバーを Rust の型として持ち、ルートごとにメソッドを一つずつ提供します |
| `crates/store` | agency のテーブル、そのマイグレーション、それに対するステートメント |
| `crates/tool-call` | ツールホストの呼び出しプロトコルの agency 側。agency のツールが共有します |
| `crates/server` | サービス本体: セッション、場所、ライン、ロールスロット、制御ルート、動詞。バイナリは `agency-server` と `bmo-agency` |
| `tools/` | agency がツールホストに提供するツール: 手続き群とルールブックのリーダー |
| `packages/` | TypeScript パッケージ（一つの Bun ワークスペース） |
| `screen-kit/` | 人の PC で個人アプリの画面をビルドするもの：コンソールと同じリリースの bmo-ui バンドルビルド |
| `policy/`, `prompts/` | agency が従う固定値とプロンプト |
| `deploy/` | サービスユニット: デプロイのたびに再起動される agency サーバー、エージェントホスト、モデルゲートウェイと、agency のデプロイでは保持しない herdr のユニット |
| `testbed.toml` | テストベッドの宣言: リポジトリのテスト、チェック、ビルド、デプロイの方法。デプロイ先は、共有するデータルート `~/.local/share/bmo/data` の隣にある agency 自身のデプロイツリー `~/.local/share/bmo-agency` です |
| `install.toml` | インストール宣言: 新しいマシンに agency をインストールするのに必要なもので、`bmo install` が読みます |
| `tests/` | すべてのテスト |

## コマンド

```sh
cargo check --workspace --all-targets
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
(cd packages && bun install && bunx tsc --noEmit)
(cd tests/packages && bun test)
```

## ライセンス

リポジトリ自身のコードは MIT ライセンスで、その全文はルートの `LICENSE` にあります。`cache-siren` クレートは
ライセンスを確認できなかった上流プロジェクトから派生したものであるため、ライセンス表記を持たず、リポジトリの
MIT ライセンスの対象外です。

## バージョン

バージョンはセット全体のもので semver に従い、ビルド時に git タグから読み取ります。ワークスペースの
マニフェストのバージョンは動かないプレースホルダーです。
