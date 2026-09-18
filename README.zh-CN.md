<div align="center">
  <img src="app-icon.svg" width="96" alt="VibeShell" />
  <h1>VibeShell</h1>
  <p><strong>你的终端，你的 Agent，同一个工作区。</strong></p>
  <p>为人和编程 Agent 共同使用而设计的本地优先 SSH/SFTP 工作区：操作看得见，会话能共享，文件不脱节，插件可按需发现。</p>

  [English](README.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md)

  [![CI](https://github.com/veithly/vibeshell/actions/workflows/ci.yml/badge.svg?branch=dev)](https://github.com/veithly/vibeshell/actions/workflows/ci.yml)
  [![Release](https://img.shields.io/github/v/release/veithly/vibeshell)](https://github.com/veithly/vibeshell/releases)
  [![GPLv3](https://img.shields.io/badge/License-GPLv3-blue.svg)](LICENSE)

  [下载安装](https://github.com/veithly/vibeshell/releases) · [1.1 更新记录](CHANGELOG.md) · [Agent 使用入口](skills/vibeshell/SKILL.md) · [向 dev 贡献](CONTRIBUTING.md)
</div>

![VibeShell 终端工作区](docs/assets/screenshots/terminal-workspace.png)

## 不只是再封装一个 ssh 命令

SSH 本身已经提供加密传输、认证、转发和远程执行。VibeShell 不替换协议，也不宣称让网络变快；它把 **你正在使用的会话、正在编辑的文件，以及 Agent 正在执行的操作** 放进同一个工作区。

| 只用命令行时需要自行协调的事情 | VibeShell 的做法 |
| --- | --- |
| 人和 Agent 可能使用互不关联的终端。 | 桌面、原生 CLI 和 MCP 共享已保存目标，并发现现有会话。 |
| 需要追问 Agent 刚才执行了什么。 | 操作通知条与持久历史显示命令、目标和状态。 |
| Agent 新开的连接在桌面不可见。 | 新会话自动成为独立标签，不抢走人的当前焦点。 |
| 文件、隧道和命令分散在多个工具里。 | 本地/远程文件标签、SFTP、转发和插件视图与终端并列。 |
| 每个自动化脚本都要重新理解插件用法。 | 插件提供机器可读参数和当前参考文档。 |
| 仅按文件大小比较可能漏掉同长度修改。 | 目录同步比较内容，删除多余文件时保护排除项。 |

这些体验也可以用 OpenSSH、tmux、编辑器和脚本组合出来。VibeShell 的区别是把协作本身做成产品功能，减少手工配置与上下文切换。

## 1.1 的核心体验

### 和 Agent 一起操作，也能知道它在做什么

CLI 与 MCP 的操作都会进入活动历史，记录命令、Session、时间和生命周期状态。重复运行同一命令不会被去重；多行命令完整保留，历史可以分页查看，重新打开 UI 后仍可恢复。活动记录在本地加密，不进入云同步。

需要在同一个 shell 里协作时使用共享交互终端；不想打断人的提示符时使用独立 exec，命令仍然出现在活动历史中。**输入发送成功不等于远端命令执行成功**，界面和接口区分这两种语义。

Agent 新建会话后，UI 自动补上标签，但保留人的当前选择。同一服务器的多个连接按 Session 身份分别管理。会话能否在关闭 GUI 后继续存在取决于所属进程：daemon 持有的会话可以继续运行，GUI 持有的连接不会在退出其所属进程后凭空保留。

### 少管理窗口，多保留上下文

统一连接启动器支持搜索、列表/卡片视图，将 SSH、本地 shell 和编程 Agent 入口放在一起。提供键盘焦点管理、明暗主题与减少动态效果设置；卡片动画使用浏览器原生能力。

终端与文件可以分屏、独立窗口展示。调整布局时保留尚未保存的文件编辑，配合指令历史、片段、上下文操作和明确的错误提示，减少复制粘贴和不确定的“成功”。

![连接启动器](docs/assets/screenshots/server-launcher.png)

### 文件工作就在会话旁边

SFTP 支持分栏和图标浏览、多选、复制路径及传输进度；本地和远程文件可作为工作区标签打开，支持文本/代码、图片、PDF、媒体和归档预览。

传输优先保证完整性：有界分块读写，下载等待本地写入完成后才报告成功；同步识别同大小内容变更，删除时遵守排除规则和嵌套 `.gitignore`；本地同步拒绝重叠的源/目标目录。内容比较可能增加远程读取流量，这是完整性与流量之间的明确取舍，并非节省带宽的承诺。

![SFTP 工作流](docs/assets/screenshots/sftp-workflow.png)

### 改密码或私钥，不必删掉服务器重建

服务器编辑支持新密码、替换私钥文件/内容，以及修改或清空私钥口令。未修改字段保留旧值，不会把已有秘密读回表单。服务器资料、改名与凭据修改在同一事务中成功或回滚。

这里修改的是 **VibeShell 保存的登录信息**，不是远端系统账号的真实密码。登录私钥正确，也不代表可以绕过服务器主机身份验证；未知或变更的主机指纹仍需正确处理。

### 原生自动化，共用目标和会话

Rust 编写的 `vibeshell` 可执行文件不需要 Node.js 或桌面窗口即可运行，按需启动原生 daemon，复用已保存配置。daemon 已持有会话时，GUI 不替换其活动 socket；终端、SFTP、隧道、录制和数据库探测请求交由实际持有连接的进程执行。

本地 Agent 启动器可通过真实 PTY 启动 Claude Code、Codex、OpenCode、Pi 等工具，并展示仓库状态与差异。这些工具需单独安装和配置；VibeShell 不附带模型订阅，也不接管其账户凭据。

### 插件不只有按钮，也有 AI 接口

内置及符合规范的导入插件都提供安装状态、权限、动作参数和当前参考文档。**主 Skill 负责导航，详细用法按插件读取**，避免每次给 Agent 加载一整本手册。

```bash
vibeshell plugins list --installed --json
vibeshell plugins describe server-performance
vibeshell plugins docs server-performance
vibeshell plugins run server-performance status --session SESSION_ID --inputs '{}'
```

先确认插件已安装、已启用，并将 `SESSION_ID` 替换为真实会话。`describe` 返回机器可读参数结构；`docs` 从当前有效的插件声明生成文档，也适用于导入插件。

| 类别 | 内置插件 |
| --- | --- |
| 主机运维 | 性能、进程、系统日志、网络、磁盘 |
| 服务与基础设施 | Docker、Kubernetes、Cron、Systemd |
| 数据与开发 | 数据库、Redis、Git 工作区 |

共 12 个内置插件。CLI 和 MCP 复用启用状态、权限与输入校验，不因为文档里有示例就自动安装、授权或提权。MCP 经由真实人类审批通道确认，而不是相信模型传入的批准标志。目标机器仍需具备相应工具和权限；有插件声明不等于已经安装 Docker 或 Kubernetes。

[插件规范](docs/plugin-spec.md) · [协作与接口说明](docs/AGENT_COLLABORATION.md) · [插件参考索引](skills/vibeshell/SKILL.md#plugin-discovery-and-references)

## 从已有服务器开始

从 [Releases](https://github.com/veithly/vibeshell/releases) 选择与你平台和架构匹配的已发布版本，不使用尚未完成的草稿产物。

| 平台 | 桌面安装包 | 独立 CLI |
| --- | --- | --- |
| macOS Apple Silicon / Intel | 对应架构 `.dmg` | 对应架构 `.tar.gz` |
| Windows x64 | `.exe` / `.msi` | `.zip` |
| Linux x64 | `.AppImage` / `.deb` | `.tar.gz` |

Apple Developer ID 签名和公证情况以发布说明为准；本地 ad-hoc 签名不是 Apple 公证。移动端仍属实验性支持，不具备完整桌面功能。

独立 CLI 压缩包带有 `install.sh` 或 `install.ps1`，阅读后执行即可；详见 [CLI 安装说明](cli/README.md)。桌面包内置 CLI，Skill 安装器会向支持的 Agent 目录写入主文档和插件参考文档。

```bash
vibeshell version
vibeshell import auto --dry-run
# 先审阅预览，再正式导入。
vibeshell import auto
vibeshell servers
vibeshell ssh my-server
```

将 `my-server` 换成已保存名称。支持导入 OpenSSH、PuTTY、Tabby 配置，但刻意不复制第三方保存的密码；PuTTY `.ppk` 需先转换为 OpenSSH 格式。也可以在 GUI 中添加和编辑服务器。**CLI 新建/删除服务器与 Teleport 不属于 1.1.0 已交付功能。**

```bash
vibeshell ssh my-server -- uname -a
vibeshell sessions
# 使用上一步真实返回的别名，不一定是 001。
vibeshell ssh-session 001 -- pwd
vibeshell sftp my-server ls /srv/app
vibeshell sftp my-server get /srv/app/config.toml ./config.toml
```

复杂引号或多行脚本使用 `--command-file ./remote-command.sh` 或 `--command-stdin`。只在需要独立连接时使用 `--new`。不要把密码和私钥写进命令行参数或 Agent 提示词。

## 转发、录制和可选同步

本地转发、SOCKS5、反向转发提供监听就绪检查、取消和半关闭处理；会话结束时清理关联隧道与录制。将监听地址从回环地址改为对外开放前，应确认影响范围。

可选加密同步通过你配置的 Gist 或 WebDAV 保存服务器元数据、分组、片段和插件安装信息，不通过 VibeShell 托管的 SSH 中继。登录凭据、主机信任、活动终端与 Agent 操作历史不进入该同步。请保护提供商令牌、恢复材料和本地导出文件，也不要假定所有插件设置都不敏感。

## 安全和兼容性边界

SSH 认证前验证主机身份，经过跳板时验证真实目标。设备密钥和已保存凭据采用本地加密存储，Unix 下限制文件权限；这**不是 OS Keychain 托管**，也不能保护已经被攻陷的本机用户账户。

隔离 OpenSSH 测试覆盖常见密码、私钥、PAM keyboard-interactive、PTY、SFTP 与转发，但不等于支持所有 MFA、硬件令牌、网络设备和 SSH 实现。远程性能采样当前依赖 Linux `/proc`。

Agent 仍需要监督：风险命令识别不是沙箱。`send-secret` 可避免真正的敏感提示输入进入活动日志，但不能阻止远程程序回显；也不能用它隐藏命令。收到不确定的网络错误时，不应自动重放可能已经执行过的修改命令。

请通过 [私密安全报告](https://github.com/veithly/vibeshell/security/advisories/new) 反馈安全问题，不在公开 Issue 中上传密码、私钥或可利用的生产环境详情。

## 开发与贡献

**功能、修复、文档 PR 一律先提交到 `dev`。** `main` 是稳定发布分支，只接受本仓库 `dev` 的发布晋级。`master` 保留历史，不再作为另一个开发入口。

```bash
git clone --branch dev https://github.com/veithly/vibeshell.git
cd vibeshell
npm ci
npm run tauri -- dev
```

需要 Node.js 22.12+、当前稳定 Rust（清单最低要求 1.89），以及所在系统的 [Tauri 构建前置条件](https://v2.tauri.app/start/prerequisites/)。

```bash
node scripts/check-release.mjs
npm test
npm run build
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
# 可选：仅绑定回环地址的 Docker OpenSSH 回归。
bash scripts/test-ssh-compatibility.sh
```

构建 CLI：`cargo build --release --locked -p vshell --bin vibeshell`。构建带 sidecar 的桌面包：`npm run build:desktop`。构建不代表可以自动覆盖正在使用的应用或终止现有 SSH。

[贡献指南](CONTRIBUTING.md) · [发布流程](docs/RELEASING.md) · [架构与 Agent 开发约定](AGENTS.md)

## 许可证

VibeShell 从 **1.1.0** 起整体采用 **GNU GPL 第 3 版，仅此版本（`GPL-3.0-only`）**。参见 [LICENSE](LICENSE) 和 [NOTICE](NOTICE)。旧 MIT 版本的既有授权不被追溯撤销，[原 MIT 声明](licenses/legacy-MIT.txt) 对之前按该许可提供的代码部分予以保留；第三方组件保留各自许可与版权声明。

发布下载提供对应源码和许可声明。在法律允许范围内，本软件不提供担保。
