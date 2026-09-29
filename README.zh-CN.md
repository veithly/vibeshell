<div align="center">
  <img src="app-icon.svg" width="96" alt="VibeShell" />
  <h1>VibeShell</h1>
  <p><strong>连服务器、改文件、和 AI 一起做事，不必来回换地方。</strong></p>
  <p>一个自己用顺手、和 Agent 共用也看得明白的 SSH 工作区。</p>

  [English](README.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md)

  [![CI](https://github.com/veithly/vibeshell/actions/workflows/ci.yml/badge.svg?branch=dev)](https://github.com/veithly/vibeshell/actions/workflows/ci.yml)
  [![Release](https://img.shields.io/github/v/release/veithly/vibeshell)](https://github.com/veithly/vibeshell/releases)
  [![GPLv3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)

  [下载](https://github.com/veithly/vibeshell/releases) · [Agent / CLI 指南](skills/vibeshell/SKILL.md) · [更新记录](CHANGELOG.md) · [参与开发](CONTRIBUTING.md)
</div>

![同一工作区里的终端、主机状态与 Agent 操作历史](docs/assets/screenshots/tour-collaboration.png)

*这些是实际 VibeShell 组件的截图，填入的是虚构 Northstar 项目数据。演示在隔离浏览器里运行，没有连接真实服务器、读取凭据、调用模型或重启服务。Agent 对话和命令结果是示例，不是一次真实 Agent 执行的录像。[复现截图](scripts/readme-demo/README.md)。*

## 少一点来回复制，多一点把事情做完

一次普通的服务器排错，往往不止敲几个命令：先看日志，再找配置，切到编辑器改两行，问一下 Agent，最后还要确认它操作的是哪台机器、哪个会话。

VibeShell 想把这些事放在一起。SSH、本地终端、编程 Agent、远程文件、Git 差异和运维面板都可以成为工作区里的标签。你可以把它当作普通终端，也可以在需要时让 AI 加入；不使用 AI，并不妨碍日常终端和文件操作。

它的区别不在于重新发明 SSH，也不是宣称网络更快。用 OpenSSH、tmux、编辑器和脚本，同样能组合出很多能力。VibeShell 把它们之间的衔接做好，让你少配置、少复制、少丢上下文。

## AI 可以动手，你不用闭眼

### 它执行了什么，在哪个会话里，都能看到

桌面、原生 CLI 和 MCP 共用已保存的目标，并能发现现有会话。Agent 执行操作时，通知条和历史里会出现命令、Session、时间和状态。相同命令跑两次，留下两条记录；多行命令也能完整查看。

需要一起操作同一个提示符时，可以使用共享交互终端。不想打断你的输入时，让 Agent 独立执行检查命令，操作仍然出现在历史里。**“输入已发送”和“命令已完成”是两件事**，不会用一个含糊的成功提示混过去。

Agent 新开的会话会自动变成标签，但不会把你从当前标签拉走。同一服务器的两个连接也按 Session 分开管理。历史支持分页，重新打开 UI 后仍可读取。

### 要批准的是这条命令，不是一句“让我处理一下”

审批窗口直接展示准备执行的命令和需要确认的原因。你可以允许这一次，也可以拒绝，不必等看到输出才发现服务被重启了。CLI 和 MCP 的插件操作同样遵守各自的权限与确认要求。

![实际审批界面展示拟执行命令、风险原因和允许或拒绝入口](docs/assets/screenshots/tour-agent-approval.png)

*图中只是演示请求，没有实际执行重启。命令识别和审批不是沙箱，也不代表所有风险都能被自动识别。*

## 用你熟悉的编程 Agent，旁边就是它改的代码

VibeShell 可以通过真实本地终端启动单独安装的 **Claude Code、Codex、OpenCode、Pi** 等工具。先选项目目录，写下这次要做什么，再选择工具支持的新会话、继续最近一次或选择历史会话；访问模式也在启动前明确给出。

![编程 Agent 启动器：项目目录、会话模式、访问模式和初始提示词](docs/assets/screenshots/tour-agent-launcher.png)

Agent 工作时，不必一直相信它的口头总结。打开 **Workspace changes / 工作区变更**，旁边就能看到分支、变动文件和逐行差异。一个标签里写本地代码，另一个标签里查看远程环境，既保留上下文，也不会偷偷把本地项目和远端执行环境混为一谈。

![示例 Agent 终端旁的真实 Git 变更列表与逐行差异](docs/assets/screenshots/tour-agent-review.png)

*这些 Agent 需要各自的安装、登录和模型订阅。VibeShell 提供启动和工作区整合，不附送模型账户；图中的 Agent 输出为明确标注的演示文本。*

## 小事情也顺手，没开 AI 也一样

只是忘了一个参数，不应该先打开聊天框。内置补全可以提示命令、子命令和选项，附带说明，并结合指令历史给出候选；行内提示和键盘可操作的列表就放在光标附近。经常用的命令可以保存为片段，临时检查则用 **Quick Cmd / 快捷命令** 查看输出，不占用正在操作的交互提示符。

还可以单独开启 **AI 命令预测**，配置自己的 OpenAI 兼容接口或 Claude 接口、模型和密钥，让它补出你正在输入的后半句。它只给建议，不会替你执行；这和启动一个完整的编程 Agent 是两个入口。

**AI 预测默认关闭。** 开启后，当前输入、近期命令历史和本地补全候选会发送给你配置的提供商。不能离开本机的内容不要用于这个功能；普通补全不依赖模型接口。

终端使用 xterm.js，在可用时使用 WebGL，并对输入和输出做批处理来减少界面开销。这些是为了交互响应，不是“让 SSH 带宽翻倍”的承诺。

## 找连接，不必先想 IP

统一启动器把 **SSH、本地 Shell 和编程 Agent** 放在一起。搜索已保存的服务器，在紧凑列表和卡片之间切换，用分组与标签整理环境。已有连接的提示和单独的新会话入口，让“回到刚才的工作”和“再开一个连接”更好区分。

![可搜索的连接卡片、分组与已有会话入口](docs/assets/screenshots/tour-connections.png)

已有 OpenSSH、PuTTY、Tabby 配置可以先预览再导入；内网目标可以配置跳板机。第三方保存的密码不会被顺手复制过来，PuTTY `.ppk` 私钥需要先转换成 OpenSSH 格式。

换密码或私钥，也不必删掉服务器重建。编辑时，没动的字段保留原值，不把已有秘密读回表单；服务器资料和凭据一起保存，失败一起回滚。这里改的是 **VibeShell 保存的登录信息**，不是远端系统账号本身的密码。

## 文件就在会话旁边，本地文档也不例外

通过 SFTP 打开远程配置，或者按 **⌘/Ctrl+O** 打开本地文件，都能获得独立文件标签。本地文件不需要先建 SSH 连接；关掉最后一个终端，也不会把本地笔记一起关掉。

文本和代码可以编辑、语法高亮，Markdown 可以看源码、预览，或左右对照。SFTP 有分栏和图标浏览、多选、复制路径与上传下载进度，还可以查看支持的图片、PDF、媒体和归档。看手册、查日志、改配置，不用分别记住几个窗口的位置。

终端、文件和插件可以分屏、移动，文档也能放进独立窗口。调整布局不等于丢掉未保存内容；本地文本保存前会检查是否被其他程序改过，发现冲突就拒绝悄悄覆盖，截断读取也不会被当成完整文件保存。

目录传输同样重视“传对”：分块读写、写完才报下载完成、识别大小相同但内容不同的修改，删除多余文件时保护排除项和嵌套 `.gitignore`。比较内容可能多读一些远程数据，这是可靠性与流量的取舍。

[本地文件、Markdown 支持范围与编辑限制](docs/local-files-and-css-themes.md)

## 有时用命令，有时直接看一张表

查看容器、CPU 或数据库时，不一定每次都想读一屏原始输出。可以在当前会话旁打开插件，而不是再去另一个管理工具里填写一遍服务器地址。

| 正在处理什么 | 内置视图与工具 |
| --- | --- |
| 主机变慢、服务异常 | 性能、进程、系统日志、网络、磁盘 |
| 服务与基础设施 | Docker 容器、Kubernetes Pod、Cron、Systemd |
| 数据与代码 | 数据库、Redis、Git 工作区 |

这 **12 个内置插件** 也不是只有人能点的按钮。Agent 可以直接发现安装状态、读取动作参数、按需拿到当前用法，再通过 CLI 或 MCP 使用：

```bash
vibeshell plugins list --installed --json
vibeshell plugins describe server-performance
vibeshell plugins docs server-performance
vibeshell plugins run server-performance status --session SESSION_ID --inputs '{}'
```

先确认插件已安装、已启用，再替换真实 Session ID。主 Skill 只做导航，详细说明在 `references/<plugin-id>.md`；符合规范的导入插件也走同一套接口，文档来自当前有效的插件声明，不靠 Agent 猜命令。

读文档不等于自动授权，也不会替你安装远程软件。Docker、Kubernetes 和数据库工具仍需要目标环境及权限；远程主机性能采集当前依赖 Linux `/proc`。

[插件规范](docs/plugin-spec.md) · [Agent 和插件参考索引](skills/vibeshell/SKILL.md#plugin-discovery-and-references)

## 工作区按你的习惯来

**少管窗口，多留上下文。** 终端、文档和插件可以分屏、重新排列、移到独立窗口，还能保存布局再回来。恢复布局不等于断掉的网络连接可以跨进程重启继续存在。

**长时间工作，也要舒服。** 明暗主题、跟随系统外观、终端字体与光标设置、键盘导航、减少动态效果都在。应用界面有英文和简体中文；日文 README 是文档翻译，不代表已有日文 UI。

**不只换几个颜色。** 自定义 CSS 支持即时预览、应用保存、导入导出和本地背景图片，可以改间距、圆角、文档排版。如果主题把按钮藏没了，**⌘/Ctrl+Shift+F12** 或原生菜单里的 *Disable Custom CSS* 可以停用它。只应用可信主题，CSS 里的远程 URL 也会产生网络请求。

[自定义 CSS 与恢复方法](docs/local-files-and-css-themes.md) · [主题起点](themes/vibecode-starter.css)

## 该有的 SSH 工具，没有丢

本地转发、SOCKS5、反向转发，以及会话录制和回放都在。保存隧道配置，减少重复设置；会话结束也会清理关联隧道和录制。把监听地址从回环改为对外开放之前，先确认影响范围。

可选的 **Gist / WebDAV 加密同步** 可以同步服务器元数据、分组、片段和插件安装信息，方便在自己的设备之间延续设置。它不是 VibeShell 托管的 SSH 中继。登录凭据、主机信任、活动终端和 Agent 操作历史不进入这份同步；提供商令牌、恢复材料与导出内容仍需自己妥善保护。

SSH 在认证前检查服务器身份，经过跳板时也验证实际目标。凭据和 Agent 操作历史采用本地加密存储，但不是 OS Keychain 托管，也不能抵挡已被攻陷的本机账户。私钥正确，并不是接受陌生服务器指纹的理由。

## 从你已有的服务器开始

在 [Releases](https://github.com/veithly/vibeshell/releases) 下载对应平台的桌面包或独立 CLI。

| 平台 | 桌面安装包 | 原生 CLI |
| --- | --- | --- |
| macOS Apple Silicon / Intel | 对应架构 `.dmg` | `.tar.gz` |
| Windows x64 | `.exe` / `.msi` | `.zip` |
| Linux x64 | `.AppImage` / `.deb` | `.tar.gz` |

桌面内置 CLI；独立 CLI 压缩包附带 `install.sh` / `install.ps1`，请阅读后执行。Rust 原生 CLI 本身不依赖 Node.js。[CLI 安装说明](cli/README.md)

```bash
vibeshell import auto --dry-run    # 审阅后去掉 --dry-run 正式导入。
vibeshell servers
vibeshell ssh my-server
vibeshell ssh my-server -- uname -a
vibeshell sessions
vibeshell sftp my-server ls /srv/app
```

把 `my-server` 换成已保存名称。使用 `sessions` 返回的别名执行 `vibeshell ssh-session ALIAS -- pwd`；需要另开连接时再加 `--new`。复杂引号或多行脚本使用 `--command-file` / `--command-stdin`。不要把密码和私钥写进参数或 Agent 提示词。

CLI 可按需启动 daemon，GUI 能接入已有会话。连接依赖实际持有它的进程：daemon 的会话可在 GUI 关闭后继续，GUI 自己持有的连接则会随该进程退出而结束。升级时应让桌面与 CLI 保持一致，重启前保存正在做的工作。

Apple 签名和公证以每次发布说明为准，ad-hoc 签名不是 Apple 公证。移动端仍是实验性支持，常见 OpenSSH 测试也不能覆盖每一种 MFA、硬件令牌或 SSH 实现。提议中的 CLI 新建/删除服务器和 Teleport 不属于当前 1.1.0 发布内容。

## 自己运行，或者参与开发

项目使用 Tauri 2、Rust、React、TypeScript 和 xterm.js。开发需要 Node.js 22.12+、当前稳定 Rust，以及系统对应的 [Tauri 前置依赖](https://v2.tauri.app/start/prerequisites/)。

```bash
git clone --branch dev https://github.com/veithly/vibeshell.git
cd vibeshell
npm ci
npm run tauri -- dev
```

提交前运行 `node scripts/check-release.mjs`、`npm test`、`npm run build`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets --locked -- -D warnings` 和 `cargo test --workspace --locked`。SSH 变更还可运行仅绑定回环地址的 Docker 夹具：`bash scripts/test-ssh-compatibility.sh`，不要拿自己的真实凭据库做回归测试。

**普通 PR 一律先到 `dev`。** `main` 只接受本仓库 `dev` 的发布晋级，`master` 保留历史。构建代码不等于允许覆盖正在使用的应用或结束别人的 SSH 会话。

[贡献指南](CONTRIBUTING.md) · [架构](AGENTS.md) · [发布流程](docs/RELEASING.md) · [协作接口](docs/AGENT_COLLABORATION.md)

安全问题请走 [私密报告](https://github.com/veithly/vibeshell/security/advisories/new)，不要在 Issue 里贴真实凭据。审批不是沙箱，敏感输入保护也不能阻止远端程序回显；响应丢失后，不应自动重放可能已执行的修改命令。

## 许可证

VibeShell 从 1.1.0 起整体采用 **GPL-3.0-only**。参见 [LICENSE](LICENSE)、[NOTICE](NOTICE) 和 [保留的 MIT 声明](licenses/legacy-MIT.txt)。旧 MIT 授权不追溯撤销，第三方组件保留自己的许可证；发布下载提供对应源码及声明。在法律允许范围内，软件不提供担保。
