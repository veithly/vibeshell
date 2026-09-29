<div align="center">
  <img src="app-icon.svg" width="96" alt="VibeShell" />
  <h1>VibeShell</h1>
  <p><strong>サーバーも、ファイルも、AI との作業も、同じ場所に。</strong></p>
  <p>自分で使いやすく、Agent と一緒に使っても作業を見失わない SSH ワークスペース。</p>

  [English](README.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md)

  [![CI](https://github.com/veithly/vibeshell/actions/workflows/ci.yml/badge.svg?branch=dev)](https://github.com/veithly/vibeshell/actions/workflows/ci.yml)
  [![Release](https://img.shields.io/github/v/release/veithly/vibeshell)](https://github.com/veithly/vibeshell/releases)
  [![GPLv3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)

  [ダウンロード](https://github.com/veithly/vibeshell/releases) · [Agent / CLI ガイド](skills/vibeshell/SKILL.md) · [変更履歴](CHANGELOG.md) · [開発への参加](CONTRIBUTING.md)
</div>

![ターミナル、ホストの状態、Agent の操作履歴をまとめたワークスペース](docs/assets/screenshots/tour-collaboration.png)

*実際の VibeShell コンポーネントに、架空の Northstar プロジェクトのデータを入れた画面です。隔離したブラウザで表示しており、実サーバー接続、認証情報の読み出し、モデル呼び出し、サービス再起動は行っていません。Agent の会話や実行結果も説明用の例で、実際の Agent 実行の記録ではありません。[再現方法](scripts/readme-demo/README.md)。*

## ツール間で説明し直す時間を減らす

サーバーの調査は、コマンドを数個打つだけでは終わりません。ログを見て、設定を探し、エディタで修正し、Agent に相談する。そのたびに「どのマシンの、どのセッションか」を確認する必要があります。

VibeShell は SSH、ローカルターミナル、コーディング Agent、リモートファイル、Git の差分、運用パネルを同じタブ付きワークスペースにまとめます。普通のターミナルとして使い、必要な場面だけ AI を加えられます。通常の端末・ファイル操作にモデルの契約は必要ありません。

新しい SSH プロトコルでも、通信速度を上げる仕組みでもありません。OpenSSH、tmux、エディタ、スクリプトでも多くの同じ作業はできます。VibeShell が減らしたいのは、その間の設定、コピー、ウィンドウ切り替えです。

## Agent に任せても、何をしているか分かる

### コマンドと対象セッションを見える場所に

GUI、ネイティブ CLI、MCP は保存済みサーバーを共有し、既存セッションを発見できます。Agent の操作は通知と履歴に現れ、コマンド、セッション、時刻、状態を確認できます。同じコマンドの再実行は別の記録として残り、複数行の内容も確認できます。

同じプロンプトで一緒に作業するときは共有ターミナルへ入力し、人の作業を邪魔せず調査するときは独立した実行を使えます。どちらも履歴に表示されますが、**入力送信とコマンドの正常終了は区別されます。**

Agent が別のセッションを作ると新しいタブに反映されます。人が選んだタブのフォーカスは奪いません。同じサーバーへの複数接続もセッションとして別々に管理し、履歴はページ送りや UI 再起動後の読み出しに対応します。

### 承認するのは、曖昧な依頼ではなく具体的な操作

承認画面には、実行予定のコマンドと確認が必要な理由を表示します。その一回を許可するか、拒否するかを判断でき、あとからログで再起動を知る必要はありません。CLI と MCP のプラグイン操作にも権限と確認のチェックがあります。

![実行予定のコマンドと確認理由を示す Agent 承認画面](docs/assets/screenshots/tour-agent-approval.png)

*画像はデモの承認要求です。再起動は実行していません。コマンド分類と承認はサンドボックスではなく、あらゆるリスクを自動検知する保証でもありません。*

## いつものコーディング Agent と、隣にある差分

別途インストールした **Claude Code、Codex、OpenCode、Pi** などを、実際のローカルターミナルで起動できます。作業ディレクトリと最初の依頼を設定し、選択したツールが対応する新規・直前の続き・過去のセッション選択を使えます。アクセスモードも起動前に明示されます。

![作業ディレクトリ、セッションとアクセスモード、最初の依頼を設定する画面](docs/assets/screenshots/tour-agent-launcher.png)

Agent の説明だけで変更を判断する必要はありません。**Workspace changes** を開くと、ブランチ、変更ファイル、行ごとの差分をターミナルの隣で確認できます。ローカル開発とリモート作業を隣り合うタブに置きつつ、実行環境まで同じものとして扱うことはありません。

![説明用 Agent 出力の隣に表示した実際の Git 変更一覧と差分](docs/assets/screenshots/tour-agent-review.png)

*各 Agent のインストール、ログイン、モデル契約は別途必要です。VibeShell が提供するのは起動と作業環境の統合であり、モデルの利用権や実行済み成果の保証ではありません。*

## 小さな操作は、AI なしでも快適に

オプションを一つ忘れただけなら、チャットを開く必要はありません。内蔵の補完はコマンド、サブコマンド、オプションを説明付きで提案し、履歴も候補に使います。行内の候補とキーボード操作可能なリストをカーソルの近くに表示します。定型コマンドはスニペットに、短い調査は **Quick Cmd** に任せれば、対話中のプロンプトを占有せず結果を見られます。

追加の支援が欲しい場合は、自分の OpenAI 互換または Claude エンドポイントとモデルを設定して **AI コマンド予測** を有効化できます。入力中の後半を提案する機能で、自動実行はしません。コーディング Agent を起動する機能とは別です。

**予測は初期状態で無効です。** 有効にすると、現在の入力、最近のコマンド履歴、ローカル補完候補を設定先へ送信します。外部に出せない作業では無効のまま使ってください。通常の補完はモデル API に依存しません。

ターミナルは xterm.js を使用し、利用可能なら WebGL で描画します。入出力のバッチ処理も UI の負荷を減らすためのもので、SSH 回線が速くなるという主張ではありません。

## IP アドレスより先に、作業先を見つける

接続ランチャーは **SSH、ローカル Shell、コーディング Agent** の共通入口です。サーバー検索、リスト/カード表示、グループやタグによる整理に対応します。既存セッションの表示と新規接続用の操作を分け、前の作業に戻る場合と別接続を作る場合を区別できます。

![グループと既存セッションを確認できる接続カード](docs/assets/screenshots/tour-connections.png)

OpenSSH、PuTTY、Tabby の設定をプレビューしてから取り込み、プライベートな接続先には踏み台を設定できます。他製品に保存されたパスワードはコピーしません。PuTTY `.ppk` は OpenSSH 形式への変換が必要です。

パスワード、秘密鍵、パスフレーズを変えるためにサーバーを作り直す必要もありません。変更しない値を保ち、既存の秘密情報は編集フォームへ読み戻さず、接続情報と認証情報をまとめて保存またはロールバックします。対象は **VibeShell の保存済みログイン情報**であり、リモート OS のパスワードそのものではありません。

## コマンドの隣に、必要なファイルを

SFTP でリモート設定を開いたり、**⌘/Ctrl+O** でローカルファイルを開いたりできます。ローカル文書のために SSH 接続を作る必要はなく、最後のターミナルを閉じても文書タブは残ります。

テキスト/コード編集、構文強調、Markdown のソース・プレビュー・左右比較に対応します。SFTP にはカラム/アイコン表示、複数選択、パスコピー、転送進捗、対応する画像・PDF・メディア・アーカイブの表示機能があります。手順書、ログ、設定を別々のアプリで探し直す手間を減らせます。

ファイルとターミナルを分割・移動し、文書を別ウィンドウに出しても、未保存の編集内容を保持します。ローカルテキストの保存は外部変更を検出すると黙って上書きせず、途中までしか読めていない内容を完全なファイルとして保存することもありません。

フォルダ転送も整合性を重視します。有界チャンク、ローカル書き込みを待つ完了処理、同サイズの内容変更の検出、不要ファイル削除時の除外パスとネストした `.gitignore` の保護を備えます。内容比較は追加のリモート読み出しを伴う場合があり、帯域削減の保証ではありません。

[ローカルファイルと Markdown の対応範囲](docs/local-files-and-css-themes.md)

## コマンドが便利なときも、一覧で見たいときも

コンテナや CPU、データベースを確認するたびに、生の出力を読みたいとは限りません。別ツールでサーバーを登録し直す代わりに、使用中のセッションでプラグインを開けます。

| 作業 | 内蔵の表示とツール |
| --- | --- |
| ホストの遅延や異常 | Server Performance、Process Explorer、System Logs、Network Inspector、Disk Usage |
| サービスと基盤 | Docker Containers、Kubernetes Pods、Cron Scheduler、Systemd Services |
| データと開発 | Database Inspector、Redis Inspector、Git Workspace |

**12 個の内蔵プラグイン**は人が押すボタンだけではありません。Agent はインストール状態、入力仕様、現在の使い方を取得し、CLI または MCP から利用できます。

```bash
vibeshell plugins list --installed --json
vibeshell plugins describe server-performance
vibeshell plugins docs server-performance
vibeshell plugins run server-performance status --session SESSION_ID --inputs '{}'
```

有効な既存セッションを指定し、インストール/有効化状態を確認してください。主 Skill は索引にとどめ、詳細は `references/<plugin-id>.md` に置きます。対応する宣言型のインポートプラグインも同じ発見インターフェースを持ち、文書は現在の検証済み manifest から生成します。

文書を読むだけで権限が付いたり、必要なソフトウェアがインストールされたりすることはありません。Docker、Kubernetes、データベースの利用には対象環境と権限が必要です。ホストのリモート性能収集は現在 Linux `/proc` を前提とします。

[プラグイン仕様](docs/plugin-spec.md) · [Agent とプラグインの索引](skills/vibeshell/SKILL.md#plugin-discovery-and-references)

## 自分の作業に合わせて整える

**ウィンドウの山ではなく、作業のまとまりを。** ターミナル、文書、プラグインを分割・並べ替え・別ウィンドウ化し、レイアウトを保存できます。レイアウトの復元は、所有プロセスを終了したネットワーク接続の継続を保証するものではありません。

**長時間でも使いやすく。** 明暗テーマ、システム外観への追従、端末フォントとカーソル、キーボード操作、動きを減らす設定に対応します。アプリ UI は英語と簡体字中国語です。この日本語 README は文書の翻訳であり、日本語 UI 対応を意味しません。

**色のプリセットだけで終わらない。** カスタム CSS はライブプレビュー、適用保存、インポート/エクスポート、ローカル背景画像に対応し、余白、角丸、文書の文字組みも調整できます。テーマが操作部を隠した場合は **⌘/Ctrl+Shift+F12** またはネイティブの *Disable Custom CSS* メニューで無効にできます。CSS の外部 URL は通信を発生させるため、信頼するテーマだけを適用してください。

[CSS と復旧方法](docs/local-files-and-css-themes.md) · [スターターテーマ](themes/vibecode-starter.css)

## SSH の基本機能も、そのまま

ローカル転送、SOCKS5、リバース転送、セッションの記録と再生に対応します。トンネル設定を保存し、セッション終了時には関連トンネルと記録も終了します。loopback の外へ公開する前に bind アドレスを確認してください。

任意の **Gist / WebDAV 暗号化同期**で、サーバーメタデータ、グループ、スニペット、プラグイン導入情報を自分の環境間で共有できます。VibeShell のホスト型 SSH 中継ではありません。認証情報、ホスト鍵の信頼情報、実行中ターミナル、Agent 履歴は同期対象外です。提供元のトークン、復旧材料、エクスポート内容は慎重に管理してください。

SSH は認証前にホストを検証し、踏み台経由でも実際の接続先を確認します。認証情報と Agent 履歴はローカル暗号化保存ですが、OS Keychain 保管や侵害済みローカルアカウントからの保護ではありません。正しい秘密鍵があっても、想定外のホスト指紋を無条件で許可してはいけません。

## いつものサーバーから始める

[Releases](https://github.com/veithly/vibeshell/releases) から対応するデスクトップまたは CLI を取得できます。

| プラットフォーム | デスクトップ | ネイティブ CLI |
| --- | --- | --- |
| macOS Apple Silicon / Intel | アーキテクチャ別 `.dmg` | `.tar.gz` |
| Windows x64 | `.exe` / `.msi` | `.zip` |
| Linux x64 | `.AppImage` / `.deb` | `.tar.gz` |

デスクトップは CLI を同梱します。独立 CLI の `install.sh` / `install.ps1` は内容を読んでから実行してください。Rust の CLI 自体は Node.js に依存しません。[CLI インストール](cli/README.md)

```bash
vibeshell import auto --dry-run    # 確認後に --dry-run を外して取り込みます。
vibeshell servers
vibeshell ssh my-server
vibeshell ssh my-server -- uname -a
vibeshell sessions
vibeshell sftp my-server ls /srv/app
```

`my-server` は保存済み名に置き換えます。`sessions` に表示された別名で `vibeshell ssh-session ALIAS -- pwd` を実行し、別接続が必要な場合だけ `--new` を使います。複雑な引用符には `--command-file` / `--command-stdin` を使用し、秘密情報を引数や Agent の依頼文に入れないでください。

CLI は必要に応じて daemon を起動し、GUI は既存セッションに接続できます。接続の所有プロセスは生きている必要があります。daemon 所有の接続は GUI 終了後も継続できますが、GUI 所有の接続はその終了で切れます。デスクトップと CLI は一緒に更新し、再起動前に作業を保存してください。

Apple 署名/公証の状況は各リリースノートを参照してください。ad-hoc 署名は Apple 公証ではありません。モバイルは実験的で、全 MFA、ハードウェアトークン、SSH 実装への対応を保証しません。提案中の CLI サーバー作成/削除と Teleport は現在の 1.1.0 には含まれません。

## 動かす、開発する、実装を読む

Tauri 2、Rust、React、TypeScript、xterm.js で構成しています。Node.js 22.12+、stable Rust、OS ごとの [Tauri 前提条件](https://v2.tauri.app/start/prerequisites/) が必要です。

```bash
git clone --branch dev https://github.com/veithly/vibeshell.git
cd vibeshell
npm ci
npm run tauri -- dev
```

PR 前に `node scripts/check-release.mjs`、`npm test`、`npm run build`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings`、`cargo test --workspace --locked` を実行します。SSH 変更には loopback 限定の Docker テスト `bash scripts/test-ssh-compatibility.sh` もあります。本物の認証情報ストアをテストに使わないでください。

**通常の PR は `dev` 宛てです。** `main` は本リポジトリの `dev` からの検証済みリリース昇格を受け付け、`master` は履歴用です。ビルド操作は使用中アプリの置換や SSH 切断を許可するものではありません。

[貢献ガイド](CONTRIBUTING.md) · [アーキテクチャ](AGENTS.md) · [リリース手順](docs/RELEASING.md) · [共同作業 API](docs/AGENT_COLLABORATION.md)

セキュリティ上の問題は [非公開報告](https://github.com/veithly/vibeshell/security/advisories/new) へ。承認はサンドボックスではなく、保護された入力もリモートプログラムのエコーを防げません。応答消失後に変更操作を自動で再実行しないでください。

## ライセンス

VibeShell 1.1.0 以降の全体は **GPL-3.0-only** です。[LICENSE](LICENSE)、[NOTICE](NOTICE)、[保存された MIT 通知](licenses/legacy-MIT.txt) を参照してください。既存の MIT 許諾は取り消さず、第三者はそれぞれのライセンスを保持します。リリースには対応ソースと通知を用意し、法律で許される範囲で無保証とします。
