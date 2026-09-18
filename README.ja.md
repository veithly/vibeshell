<div align="center">
  <img src="app-icon.svg" width="96" alt="VibeShell" />
  <h1>VibeShell</h1>
  <p><strong>あなたのターミナルと Agent を、同じワークスペースに。</strong></p>
  <p>人とコーディング Agent のためのローカルファーストな SSH/SFTP ワークスペース。操作を可視化し、セッション、ファイル、プラグインをつなぎます。</p>

  [English](README.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md)

  [![CI](https://github.com/veithly/vibeshell/actions/workflows/ci.yml/badge.svg?branch=dev)](https://github.com/veithly/vibeshell/actions/workflows/ci.yml)
  [![Release](https://img.shields.io/github/v/release/veithly/vibeshell)](https://github.com/veithly/vibeshell/releases)
  [![GPLv3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)

  [ダウンロード](https://github.com/veithly/vibeshell/releases) · [1.1 の変更点](CHANGELOG.md) · [Agent ガイド](skills/vibeshell/SKILL.md) · [開発への参加](CONTRIBUTING.md)
</div>

![ターミナルワークスペース](docs/assets/screenshots/terminal-workspace.png)

## SSH の周辺作業を、一つの場所で

暗号化通信、認証、転送、リモート実行は SSH 自体の機能です。VibeShell はプロトコルを置き換えたり、ネットワーク速度の向上を約束したりするものではありません。**作業中のセッション、編集中のファイル、Agent が実行した操作**を同じ画面とデータモデルで扱います。

| コマンドラインだけでは個別に調整する作業 | VibeShell |
| --- | --- |
| 人と Agent が別々の端末を使う | GUI、ネイティブ CLI、MCP から保存済みの接続先とセッションを共有 |
| Agent の直前の操作を確認する | コマンド、対象、状態を通知と永続履歴で確認 |
| Agent の新しい接続を見つける | 新規セッションを別タブとして表示し、現在のタブのフォーカスを保持 |
| ファイルやトンネルのためにツールを切り替える | ファイル、SFTP、転送、プラグインをターミナルの隣で操作 |
| 自動化にプラグインの使い方を教える | アクションの入力スキーマと最新の参照文書を取得 |
| 同じサイズの変更を同期で見落とす | 内容を比較し、削除時には除外パスを保護 |

OpenSSH、tmux、エディタ、スクリプトでも同様の環境は構築できます。VibeShell は、その連携を最初から使えるワークフローとして提供します。

## 1.1 のワークフロー

### Agent の作業を見失わない

CLI と MCP の操作履歴には、コマンド、セッション、時刻、開始・成功・失敗の状態が残ります。同じコマンドを複数回実行しても別の記録として保持し、複数行コマンド、ページ送り、UI の再起動後の読み出しに対応します。履歴はローカルで暗号化され、クラウド同期には含まれません。

同じシェルで作業する場合は共有ターミナルを使い、人のプロンプトに干渉したくない確認作業には独立した exec を使えます。exec も履歴に表示されます。**入力送信の成功は、リモートコマンドの正常終了を意味しません。**

Agent の新規セッションは自動的にタブへ反映されますが、人が選択中のタブを切り替えません。同じサーバーへの複数接続もセッション ID で区別します。接続は所有プロセスに依存します。daemon 所有のセッションは GUI 終了後も継続できますが、GUI 所有の接続はその GUI プロセス終了後には維持できません。

### ウィンドウより、作業内容に集中

検索可能な接続ランチャーに SSH、ローカルシェル、Agent の起動入口をまとめています。リスト/カード表示、キーボードのフォーカス管理、明暗テーマ、動きを減らす設定に対応します。カードのアニメーションはブラウザ標準 API を使います。

ターミナルとファイルを分割したり、別ウィンドウに移したりできます。レイアウト変更時にも未保存の編集を保持し、コマンド履歴、スニペット、コンテキスト操作、明確なエラー表示で作業の引き継ぎを助けます。

![接続ランチャー](docs/assets/screenshots/server-launcher.png)

### ファイルと接続を切り離さない

SFTP のカラム/アイコン表示、複数選択、パスのコピー、転送進捗に対応します。ローカル/リモートファイルをタブで開き、テキスト、コード、画像、PDF、メディア、アーカイブを確認できます。

転送は有界のチャンクを使い、ダウンロードはローカル書き込みの完了を待って成功を返します。同期は同サイズの内容変更も検出し、不要ファイルの削除では除外設定とネストした `.gitignore` を保護します。ローカル同期では転送元/転送先の重複を拒否します。内容比較はリモート読み出しを増やす場合があり、帯域削減の保証ではありません。

![SFTP ワークフロー](docs/assets/screenshots/sftp-workflow.png)

### 保存済み認証情報を安全に編集

サーバーを作り直さずにパスワード、秘密鍵ファイル/内容、鍵のパスフレーズを更新できます。変更しない項目は保持し、既存の秘密情報は編集フォームへ読み戻しません。サーバー情報、名前変更、認証情報の保存は一つのトランザクションで成功またはロールバックします。

変更対象は **VibeShell に保存されたログイン情報**であり、リモート OS のアカウントパスワードそのものではありません。正しい秘密鍵があっても、未知/変更されたホスト鍵の確認は省略できません。

### ネイティブ CLI と共有セッション

Rust の `vibeshell` バイナリは Node.js やデスクトップウィンドウなしで動作し、必要に応じて daemon を起動します。既存 daemon の接続を GUI が見つけた場合、使用中の socket を置き換えず、ターミナル、SFTP、トンネル、録画、DB 検出を実際の所有プロセスへ振り分けます。

Claude Code、Codex、OpenCode、Pi などを実際の PTY で起動し、リポジトリ状態と差分も確認できます。各 Agent は別途インストールと設定が必要で、モデルの契約や認証情報は付属しません。

### Agent が直接発見できるプラグイン

内蔵プラグインと対応する宣言型インポートプラグインは、インストール状態、権限、アクションの入力仕様、最新の使い方を公開します。主 Skill は索引に留め、詳細は必要なプラグインの参照文書から取得します。

```bash
vibeshell plugins list --installed --json
vibeshell plugins describe server-performance
vibeshell plugins docs server-performance
vibeshell plugins run server-performance status --session SESSION_ID --inputs '{}'
```

プラグインがインストール/有効化されていることを確認し、`SESSION_ID` を実在する値に置き換えます。`describe` は機械可読の入力スキーマ、`docs` は現在の検証済み manifest に対応した Markdown を返します。

| 分野 | 12 個の内蔵プラグイン |
| --- | --- |
| ホスト管理 | Performance、Process、System Logs、Network、Disk Usage |
| サービス/基盤 | Docker、Kubernetes、Cron、Systemd |
| データ/開発 | Database、Redis、Git Workspace |

CLI と MCP は共通の状態/権限/入力検証を使います。使用例を読んだだけではインストール、権限付与、sudo は実行しません。MCP の承認は人の確認経路を通し、モデル自身の承認フラグを信用しません。対象マシンには対応ツールと権限が必要です。

[プラグイン仕様](docs/plugin-spec.md) · [共同作業と API](docs/AGENT_COLLABORATION.md) · [参照文書の索引](skills/vibeshell/SKILL.md#plugin-discovery-and-references)

## インストールと最初の接続

[Releases](https://github.com/veithly/vibeshell/releases) から公開済みの対応パッケージを選びます。未完成のドラフトは使用しません。

| プラットフォーム | デスクトップ | 独立 CLI |
| --- | --- | --- |
| macOS Apple Silicon / Intel | アーキテクチャ別 `.dmg` | `.tar.gz` |
| Windows x64 | `.exe` / `.msi` | `.zip` |
| Linux x64 | `.AppImage` / `.deb` | `.tar.gz` |

Apple Developer ID 署名と公証の有無はリリースノートを確認してください。ad-hoc 署名は Apple の公証ではありません。モバイル対応は実験的で、デスクトップと同じ機能範囲ではありません。

CLI アーカイブの `install.sh` / `install.ps1` は内容を確認して実行してください。[CLI ガイド](cli/README.md) に詳細があります。デスクトップには CLI が同梱され、Skill インストーラーが主文書とプラグイン参照文書を対応 Agent ディレクトリに配置します。

```bash
vibeshell version
vibeshell import auto --dry-run
# プレビューを確認してからインポートします。
vibeshell import auto
vibeshell servers
vibeshell ssh my-server
```

`my-server` は保存済みの名前に置き換えます。OpenSSH、PuTTY、Tabby のプロファイルを取り込めますが、他製品に保存されたパスワードはコピーしません。PuTTY `.ppk` は OpenSSH 形式に変換してください。GUI からも追加/編集できます。CLI のサーバー作成/削除と Teleport は 1.1.0 に含まれません。

```bash
vibeshell ssh my-server -- uname -a
vibeshell sessions
# 実際に返された別名を使用します。必ずしも 001 ではありません。
vibeshell ssh-session 001 -- pwd
vibeshell sftp my-server ls /srv/app
vibeshell sftp my-server get /srv/app/config.toml ./config.toml
```

複雑な引用符や複数行コマンドは `--command-file ./remote-command.sh` / `--command-stdin` を使います。`--new` は別の接続が必要な場合だけ指定し、秘密情報は引数やプロンプトに書かないでください。

## 転送、記録、任意の暗号化同期

ローカル転送、SOCKS5、リバース転送はリスナー準備、キャンセル、半閉鎖を扱い、セッション終了時に関連トンネルと記録も終了します。loopback 以外へ公開する前に bind アドレスを確認してください。

任意の Gist/WebDAV 同期はサーバーメタデータ、グループ、スニペット、プラグイン導入情報を暗号化します。VibeShell のホスト型 SSH 中継はありません。ログイン認証情報、ホスト鍵の信頼情報、稼働中セッション、Agent 履歴は同期対象外です。トークン、復旧材料、ローカルエクスポートを保護し、プラグイン設定にも機密情報があり得ることに注意してください。

## 安全性と対応範囲

認証前にホスト鍵を確認し、踏み台経由でも実際の宛先を検証します。ローカル認証情報は暗号化され、Unix ではファイル権限を制限します。OS Keychain 保存や、侵害されたユーザーアカウントからの保護を保証するものではありません。

隔離 OpenSSH テストは一般的なパスワード、鍵、PAM keyboard-interactive、PTY、SFTP、転送を対象とします。全 MFA、ハードウェアトークン、ネットワーク機器、SSH 実装の互換性を示すものではありません。リモート性能収集は Linux `/proc` を前提とします。

危険なコマンドの分類はサンドボックスではありません。`send-secret` は本当の機密入力を履歴から除外できますが、サーバー側のエコーは防げず、コマンドを隠すために使ってはいけません。不明な応答消失後に変更操作を自動再実行しないでください。

問題は [非公開のセキュリティ報告](https://github.com/veithly/vibeshell/security/advisories/new) へ。秘密鍵や本番環境の機密情報を公開 Issue に投稿しないでください。

## 開発と貢献

機能、修正、文書の **PR はすべて `dev` 宛て**です。`main` は安定版で、本リポジトリの `dev` からのリリース昇格を受け付けます。`master` は履歴用です。

```bash
git clone --branch dev https://github.com/veithly/vibeshell.git
cd vibeshell
npm ci
npm run tauri -- dev
```

Node.js 22.12+、stable Rust（manifest の最小要件は 1.89）、OS ごとの [Tauri 前提条件](https://v2.tauri.app/start/prerequisites/) が必要です。

```bash
node scripts/check-release.mjs
npm test
npm run build
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
# 任意の Docker / loopback 専用テスト
bash scripts/test-ssh-compatibility.sh
```

CLI: `cargo build --release --locked -p vshell --bin vibeshell`。デスクトップ: `npm run build:desktop`。ビルドは使用中アプリの上書きや SSH の切断を許可する操作ではありません。

[貢献ガイド](CONTRIBUTING.md) · [リリース手順](docs/RELEASING.md) · [アーキテクチャ](AGENTS.md)

## ライセンス

1.1.0 以降の VibeShell 全体は **GNU GPL バージョン 3 のみ（`GPL-3.0-only`）**です。[LICENSE](LICENSE) と [NOTICE](NOTICE) を参照してください。以前の MIT リリースに付与済みの権利は取り消しません。[旧 MIT 通知](licenses/legacy-MIT.txt) は該当部分について保持し、第三者コンポーネントは各自のライセンスと著作権表示に従います。

リリースには対応するソースとライセンス通知を用意します。法律が認める範囲で、本ソフトウェアは無保証です。
