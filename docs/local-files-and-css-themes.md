# 本地文件与 CSS 主题

## 打开文件

标签栏右侧的「打开文件」按钮、`⌘O`（Windows/Linux 为 `Ctrl+O`）或系统菜单 **Workspace → Open Files** 都会打开原生多选文件对话框。关闭对话框不会新建终端。文件作为独立标签打开，可以和终端、插件一起分屏或移入独立窗口；关闭最后一个终端不会关闭本地文档。

打包的 macOS 应用声明了 Markdown、文本及常用图片的文件关联。可在 Finder 的「打开方式」选择新的 VibeShell 应用，也可执行：

```sh
open -a VibeShell '/absolute/path/notes.md'
```

文件关联排名为 Alternate，不主动抢占其他应用的默认打开方式。开发模式不会注册新的 Finder 文件关联，须使用重新构建的 `.app`。本次没有实现 Finder 文件拖入窗口，避免和现有标签拖拽争抢事件。

### 阅读与编辑

| 文件 | 支持 |
| --- | --- |
| `.md`、`.markdown`、`.mdx` | 基础 Markdown 预览、源码、源码与预览并排；源码可保存 |
| UTF-8 文本、日志、JSON、CSS、配置及无扩展名文本 | 文本编辑、语法高亮（已有语言）、`⌘/Ctrl+S` 保存 |
| SVG、PNG、JPEG、GIF、WebP、BMP、ICO、AVIF | 图片预览，使用现有缩放、旋转和适配控件；解码能力取决于平台 WebView |

Markdown 当前为不引入额外依赖的基础渲染器：支持标题、段落、简单列表、任务复选框、表格、引用、代码块、常用行内强调、链接和图片。它不是完整 CommonMark/GFM 实现，也不是 MDX、HTML 或 JavaScript 执行环境；复杂嵌套、脚注、数学公式、Mermaid 和组件执行不在此版本范围。需要原文时切换「源码」。

本地文本读取上限 4 MiB；截断内容只读，不能误保存覆盖原文件。二进制预览最多 64 MiB，Markdown 相对图片最多 8 MiB，Markdown 预览最多 512 Ki 字符。无效 UTF-8、UTF-16、含 NUL 的二进制文件会明确报错，不静默替换乱码再保存。

保存会比对加载时的原文，检测到其他程序更改后拒绝覆盖并保留编辑草稿。使用同目录临时文件替换并保留权限，属于乐观冲突检查，并非跨程序的文件锁。文件拖动不等于保存：未保存编辑继续保留在原有编辑缓冲中。

Markdown 外部图片默认不请求网络，点「加载外部图片」后才显示，并禁用 Referer。相对图片只自动尝试文档目录内的路径；这不是对任意文件系统符号链接的沙箱承诺。外部链接只能通过明确点击交给系统浏览器处理 HTTP/HTTPS URL。

## 自由编写 CSS 主题

进入 **设置 → 外观 → 自定义 CSS · Vibe Coding**。可以直接写 CSS，先预览，确认后「应用并保存」。支持导入和导出 `.css` 文件；导入只填入编辑器，不立即执行样式。`themes/vibecode-starter.css` 是可直接导入的起点。

样式可以修改整个界面的颜色、字体、间距、圆角、边框、背景、伪元素和动画。它不是 JavaScript 插件系统。以下界面钩子可作为主题起点：

```css
:root {
  /* 基础主题变量写在 root 的内联样式里，因此覆盖它们需要 !important。 */
  --tokyo-bg: #181e2c !important;
  --tokyo-fg: #dde4eb !important;
  --tokyo-blue: #83bbd6 !important;
}
.session-tabbar [role="tab"] { border-radius: 9px; }
[data-vibe-surface="markdown"] { font-size: 16px; }
[data-vibe-window="detached"] { /* 独立窗口 */ }
.terminal-viewport { /* 终端容器，不直接更改画布字符排版 */ }
```

可覆盖的颜色变量包括 `--tokyo-bg`、`--tokyo-bg-dark`、`--tokyo-bg-hl`、`--tokyo-fg`、`--tokyo-fg-dark`、`--tokyo-comment`、`--tokyo-selection`、`--tokyo-blue`、`--tokyo-on-accent`、`--tokyo-red`、`--tokyo-green`、`--tokyo-yellow`、`--tokyo-magenta`、`--tokyo-cyan` 和 `--tokyo-orange`。终端颜色同步到 xterm 色板；终端字符字体、字号和行列测量仍使用终端设置。

### 图片和装饰层

点击「插入背景图片」，选择不超过 1 MiB 的本地图片。应用会把图片转成 data URL 并写入 CSS，导出后主题可连同图片一起分享，不依赖作者机器上的文件路径。可以继续调整生成的 CSS：

```css
:root { --vibe-wallpaper-opacity: 0.12; }
/* 生成的背景使用 .app-shell::after 和独立窗口 ::after。 */
/* 装饰层务必保留 pointer-events: none，避免挡住新建和关闭按钮。 */
```

也允许自己使用 `url(...)`、渐变、`::before`、`::after` 等常规 CSS。远程 URL 会产生网络请求，只有你明确导入并应用的可信主题才应获得这种能力。CSS 文本限制约 2 MiB；导出按 UTF-8 字节计数，超限会提示。

### 预览、持久化与恢复

预览是暂时的，不写入保存状态；离开外观编辑页会撤销预览。「应用并保存」会保留到下次启动，并同步到同一应用实例的其他文档窗口。主题写入与预览文档使用不同路径，不会把文档中的 `<style>` 或 `<script>` 当成应用主题执行。

即使主题把按钮或整个页面隐藏了，也能使用 **`⌘/Ctrl+Shift+F12`** 或原生菜单 **Workspace → Disable Custom CSS** 停用样式。原生菜单不受页面 CSS 控制；停用保留 CSS 文本，方便回到编辑器修正。持久化存储写入失败时，当前窗口仍可以停用样式。

## 开发验收

```sh
npm run build
npm run test -- --reporter=dot
cargo test --manifest-path src-tauri/Cargo.toml commands::local_files::tests --lib
```

原生场景位于 `scripts/document-ui-smoke.ts`，仅在显式设置 `VIBESHELL_DOCUMENT_SMOKE=run` 时注入，正常开发和打包均关闭。它读取项目自有测试文档和图标，操作真实 WebView、原生命令及窗口间迁移，不执行终端命令。使用独立应用标识与端口启动，避免混入用户工作区：

```sh
VIBESHELL_DOCUMENT_SMOKE=run VIBESHELL_SKIP_SKILL_AUTO_INSTALL=1 npm run tauri -- dev --no-watch --config '{"identifier":"com.vibeshell.document-smoke","build":{"devUrl":"http://localhost:14732","beforeDevCommand":"node node_modules/vite/bin/vite.js --port 14732"}}'
```

不同 bundle identifier 使用独立数据库和 Agent 发现文件，不导入正式应用的旧数据库。本地 macOS 原生场景已经验证：四种文件读取与实际显示、Markdown 源码/预览并排、原生整数坐标、文件独立窗口返回、CSS 色板及窗口同步、背景点击穿透、隐藏界面后的快捷键恢复。物理鼠标跨不同缩放显示器的完整手势和 Windows/Linux 桌面仍需独立验收。
