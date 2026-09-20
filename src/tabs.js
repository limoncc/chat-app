// 工具栏纯逻辑 + IPC。站点列表通过 IPC 从 Rust 配置读取（配置式，与 sites.rs 的 SiteStore 对应）。

// 循环切换：返回当前站点在站点列表中的下一个 key（顺序循环；列表为空或找不到时保持当前）。
export function nextSiteKey(sites, currentKey) {
  if (sites.length === 0) return currentKey;
  const idx = sites.findIndex((s) => s.key === currentKey);
  if (idx === -1) return sites[0].key;
  return sites[(idx + 1) % sites.length].key;
}

// 通过 IPC 获取站点配置（{ version, default_key, sites: [{key,url,title,theme}] }）。
export async function fetchSites() {
  return await window.__TAURI_INTERNALS__.invoke("get_sites");
}

// 订阅站点配置变更（设置界面增删改/排序后触发）。
export function onSitesChanged(handler) {
  listen("sites-changed", (payload) => handler(payload));
}

// 订阅当前激活站点变更（切换站点后触发）。
export function onActiveChanged(handler) {
  listen("active-changed", (payload) => handler(payload));
}

// 通过 IPC 请求后端切换站点。
export async function activateTab(key) {
  try {
    await window.__TAURI_INTERNALS__.invoke("activate_tab", { tab: key });
    return { ok: true, key };
  } catch (e) {
    return { ok: false, key, error: String(e) };
  }
}

// 打开设置窗口。
export async function openSettings() {
  try {
    await window.__TAURI_INTERNALS__.invoke("open_settings");
    return { ok: true };
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}

// Tauri 事件监听封装（无 @tauri-apps/api 依赖，直接用 internals）。
// __TAURI_INTERNALS__ 上没有 listen 方法，须注册 transformCallback 回调后
// 调 event 插件的 plugin:event|listen 命令订阅（与 @tauri-apps/api 实现一致）。
function listen(event, handler) {
  const internals = window.__TAURI_INTERNALS__;
  if (!internals || typeof internals.invoke !== "function") return;
  try {
    internals
      .invoke("plugin:event|listen", {
        event,
        target: { kind: "Any" },
        handler: internals.transformCallback((e) => handler(e.payload)),
      })
      .catch(() => {});
  } catch {
    // webview 上下文未就绪时静默
  }
}
