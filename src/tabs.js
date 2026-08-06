// 标签栏纯逻辑 + 渲染。站点列表需与 Rust 侧 src-tauri/src/sites.rs 保持一致。
export const SITES = [
  { key: "deepseek", title: "DeepSeek" },
  { key: "chatglm", title: "ChatGLM" },
  { key: "zread", title: "Zread" },
];

export const DEFAULT_TAB = "deepseek";

// 点击标签后的新激活 key；非法 key 保持当前，避免 UI 与后端不一致。
export function nextActiveKey(tabs, currentKey, clickedKey) {
  if (!tabs.some((t) => t.key === clickedKey)) return currentKey;
  return clickedKey;
}

// 创建单个标签按钮。
export function createTabEl(site, activeKey, onClick) {
  const el = document.createElement("button");
  el.className = "tab" + (site.key === activeKey ? " active" : "");
  el.dataset.key = site.key;
  el.textContent = site.title;
  el.addEventListener("click", () => onClick(site.key));
  return el;
}

// 渲染整个标签栏。
export function renderTabs(container, sites, activeKey, onClick) {
  container.replaceChildren(...sites.map((s) => createTabEl(s, activeKey, onClick)));
}

// 仅更新激活态 class（切换后调用，避免整栏重渲染丢失点击事件）。
export function updateActiveTab(container, activeKey) {
  container.querySelectorAll(".tab").forEach((el) => {
    el.classList.toggle("active", el.dataset.key === activeKey);
  });
}

// 通过 IPC 请求后端切换标签。
export async function activateTab(key) {
  try {
    await window.__TAURI_INTERNALS__.invoke("activate_tab", { tab: key });
    return { ok: true, key };
  } catch (e) {
    return { ok: false, key, error: String(e) };
  }
}
