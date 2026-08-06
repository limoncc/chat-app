# ChatApp

单窗口桌面应用，整合 DeepSeek / ChatGLM / Zread 三个 AI 站点，顶部标签栏一键切换，各站会话与登录态独立保持（多 webview 常驻，切换不重新登录）。

基于 Tauri v2 构建，代码基底来自 `deepseek_app`（旧三个项目不再维护，见 `.gitignore`）。

## 开发

```bash
npm install          # 安装 @tauri-apps/cli
npm test             # 前端标签栏逻辑单测 (node --test)
cd src-tauri && cargo test   # Rust 单测
cargo tauri dev      # 开发运行
cargo tauri build    # 打包 (macOS .app/.dmg / Windows)
```
