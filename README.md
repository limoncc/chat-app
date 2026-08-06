# ChatApp

单窗口桌面应用，把 DeepSeek / ChatGLM / Zread 三个 AI 站点整合成一个 App，顶部切换标签即可在三站之间切换，各站会话独立保持，**切换不重新登录**。

基于 [Tauri 2](https://v2.tauri.app) 构建，代码基底来自旧项目 `deepseek_app`（三个旧项目已不维护，见 `.gitignore`）。

## 功能特性

- 单窗口 + 标签切换三个 AI 站点
- 各站登录态/聊天记录独立保持（多 webview 常驻，切换不重载）
- macOS 用原生标题栏分段控件切换，Windows 用内容区标签栏，各平台原生体验
- macOS 深色/浅色窗口外观跟随站点主题
- 系统托盘：关闭窗口最小化到托盘，托盘可恢复/退出

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
# 前端标签栏逻辑单测
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

## 平台差异说明

| 平台 | 标签切换 UI | 实现 |
|------|------------|------|
| macOS | 标题栏原生分段控件（红绿灯保留） | `src-tauri/src/mac_titlebar.rs` |
| Windows / 其他 | 内容区顶部 webview 标签栏 | `index.html` + `src/tabs.js` |

两平台共用同一套切换核心逻辑（`switch_tab`：懒加载 + 显隐切换），保证会话保持。

## 目录结构

```
chat_app/
├── index.html              # 本地标签栏页面（Windows 平台使用）
├── src/
│   ├── tabs.js             # 前端标签栏逻辑（可单测）
│   └── tabs.test.js
└── src-tauri/
    ├── Cargo.toml
    ├── tauri.conf.json     # 应用名、打包、图标配置
    ├── capabilities/       # 远程站点白名单（三站域名）
    ├── icons/              # 应用图标（chatglm 风格）
    └── src/
        ├── main.rs
        ├── lib.rs          # 窗口、多 webview 布局、托盘、主题
        ├── mac_titlebar.rs # macOS 原生分段控件（仅 macOS）
        └── sites.rs        # 三站配置 + 布局计算（含单测）
```

## 常见问题

- **切换标签会重新登录吗？** 不会。三个站点各占一个常驻 webview，切换只是显示/隐藏（位置法），页面加载状态与会话完整保留。
- **为什么用 `unstable` feature？** 单窗口内叠加多个 webview 依赖 Tauri 的 `window.add_child`（当前为 unstable API），已在 `Cargo.toml` 锁定 tauri 版本。
- **改前端代码后没生效？** `npm run tauri dev` 会自动同步，若手动构建请先执行 `npm run build`。
