# ChatApp

单窗口桌面应用，把 DeepSeek / ChatGLM / Zread / Qianwen 等 AI 站点整合成一个 App。**站点完全配置式管理**：内置设置界面 + `sites.json` 配置文件，可随时添加/修改/删除/排序站点，保存后立即生效（无需重启）。各站会话独立保持，**切换不重新登录**。

基于 [Tauri 2](https://v2.tauri.app) 构建，代码基底来自旧项目 `deepseek_app`（三个旧项目已不维护，见 `.gitignore`）。

## 功能特性

- 单窗口 + 循环切换任意 AI 站点（数量不限，配置驱动）
- 各站登录态/聊天记录独立保持（多 webview 常驻，切换不重载）
- 配置式站点管理：设置界面或直接编辑 `sites.json`，增删改/排序/设默认，保存即生效
- macOS 用原生标题栏「循环切换 + 设置」按钮，Windows 用内容区 webview 工具栏，各平台原生体验
- macOS 深色/浅色窗口外观跟随站点主题
- 系统托盘：关闭窗口最小化到托盘，托盘可恢复/退出/打开设置

## 环境要求

| 依赖 | 版本 |
|------|------|
| Node.js | ≥ 18（推荐 20+） |
| Rust | stable（rustc / cargo） |
| macOS | Xcode Command Line Tools（`xcode-select --install`） |
| Windows | Microsoft C++ Build Tools（WebView2 随系统自带） |

## 构建步骤

### 1. 安装前端依赖

```bash
npm install
```

### 2. 运行测试（可选但推荐）

```bash
# 前端工具栏逻辑单测
npm test

# Rust 后端单测
cd src-tauri && cargo test
```

### 3. 开发模式运行

```bash
npm run tauri dev
```

启动后会自动同步前端文件到 `dist/`（`beforeDevCommand`），并编译运行 App。

### 4. 打包构建

```bash
npm run tauri build
```

构建产物：

| 平台 | 路径 |
|------|------|
| macOS (.app) | `src-tauri/target/release/bundle/macos/ChatApp.app` |
| macOS (.dmg) | `src-tauri/target/release/bundle/dmg/ChatApp_<version>_<arch>.dmg` |
| Windows (.exe) | `src-tauri/target/release/bundle/nsis/ChatApp_<version>_x64-setup.exe` |

> `npm run tauri build` 会先自动执行 `npm run build` 同步前端（`beforeBuildCommand`）。

### 5. 只同步前端（手动）

```bash
npm run build
```

## 站点配置

站点列表存在应用配置目录的 `sites.json`：

| 平台 | 路径 |
|------|------|
| macOS | `~/Library/Application Support/com.chatapp.desktop/sites.json` |
| Windows | `%APPDATA%\com.chatapp.desktop\sites.json` |

两种管理方式：

1. **设置界面**：macOS 标题栏「Settings」/ Windows 工具栏「Settings」/ 托盘菜单「Settings…」/ macOS `Cmd+,`，支持添加、编辑、删除、排序站点，设置默认站点。
2. **直接编辑 `sites.json`**：手动修改后**重启应用**生效（设置界面内的修改则是即时生效）。

`sites.json` 结构：

```json
{
  "version": 1,
  "default_key": "deepseek",
  "sites": [
    { "key": "deepseek", "url": "https://chat.deepseek.com", "title": "DeepSeek", "theme": "dark" },
    { "key": "qianwen",  "url": "https://www.qianwen.com",   "title": "Qianwen",  "theme": "light" }
  ]
}
```

- `key`：站点唯一标识（字母/数字/横线/下划线），不可重复。
- `url`：必须是 `http://` 或 `https://`（支持本地开发地址如 `http://127.0.0.1:3080`）。
- `theme`：`dark` 或 `light`（仅作为启动时窗口外观；页面加载后以实际主题上报为准）。
- 首次运行会写入内置默认站点；配置损坏时自动备份为 `sites.json.bak` 并回退默认。
- 运行时通过 Tauri 的 `add_capability` 动态为每个站点注入远程 URL 白名单，新站点无需改代码即可获得页面 IPC 权限。

## 平台差异说明

| 平台 | 站点切换 UI | 实现 |
|------|------------|------|
| macOS | 标题栏原生「循环切换按钮 + 设置按钮」（红绿灯保留） | `src-tauri/src/mac_titlebar.rs` |
| Windows / 其他 | 内容区顶部 webview 工具栏（循环 + 设置） | `index.html` + `src/tabs.js` |

两平台共用同一套切换核心逻辑（`switch_tab`：懒加载 + 显隐切换），保证会话保持。站点配置变更后通过 Tauri 事件广播，两平台 UI 即时刷新。

## 目录结构

```
chat_app/
├── index.html              # 本地工具栏页面（Windows 平台使用）
├── settings.html           # 设置界面页面
├── src/
│   ├── tabs.js             # 前端工具栏逻辑（IPC 拉取站点配置，可单测）
│   ├── settings.js         # 设置界面逻辑
│   └── tabs.test.js
└── src-tauri/
    ├── Cargo.toml
    ├── tauri.conf.json     # 应用名、打包、图标配置
    ├── capabilities/       # 本地页面能力（远程站点白名单运行时动态注入）
    ├── icons/              # 应用图标（chatglm 风格）
    └── src/
        ├── main.rs
        ├── lib.rs          # 窗口、多 webview 布局、托盘、菜单、IPC 命令
        ├── mac_titlebar.rs # macOS 原生标题栏控件（仅 macOS）
        └── sites.rs        # 站点配置加载/校验/增删改（含单测）
```

## 常见问题

- **切换站点会重新登录吗？** 不会。每个站点各占一个常驻 webview，切换只是显示/隐藏（位置法），页面加载状态与会话完整保留。
- **如何添加新网站？** 打开设置界面（macOS `Cmd+,` 或标题栏「Settings」），填写 key/URL/显示名称/主题后点击「添加」，立即生效。
- **配置文件被改坏了？** 应用会自动备份为 `sites.json.bak` 并回退到内置默认站点。
- **为什么用 `unstable` feature？** 单窗口内叠加多个 webview 依赖 Tauri 的 `window.add_child`（当前为 unstable API），已在 `Cargo.toml` 锁定 tauri 版本。
- **改前端代码后没生效？** `npm run tauri dev` 会自动同步，若手动构建请先执行 `npm run build`。
