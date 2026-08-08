// 设置界面逻辑：站点增删改/排序/设默认。纯函数依赖通过参数注入，便于复用与测试。

const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);

export function initSettings(document, fetchSites, onSitesChanged) {
  const listEl = document.getElementById("site-list");
  const defaultSelect = document.getElementById("default-select");
  const form = document.getElementById("add-form");
  const errorEl = document.getElementById("error");

  let cfg = { version: 1, default_key: "", sites: [] };

  const showError = (msg) => {
    errorEl.textContent = msg || "";
  };

  const btn = (text, className, onClick) => {
    const b = document.createElement("button");
    b.textContent = text;
    if (className) b.className = className;
    b.addEventListener("click", onClick);
    return b;
  };

  // 按新顺序重排（把 from 位置的 key 移到 to）。
  async function reorder(from, to) {
    const keys = cfg.sites.map((s) => s.key);
    const [k] = keys.splice(from, 1);
    keys.splice(to, 0, k);
    try {
      await invoke("reorder_sites", { keys });
    } catch (e) {
      showError(String(e));
    }
  }

  // 行内编辑：把 info 区域换成 title/url 输入框，操作区换成保存/取消。
  function editRow(li, site) {
    const info = li.querySelector(".info");
    info.replaceChildren(
      (() => {
        const t = document.createElement("input");
        t.value = site.title;
        return t;
      })(),
      (() => {
        const u = document.createElement("input");
        u.value = site.url;
        return u;
      })(),
    );

    const ops = li.querySelector(".ops");
    ops.replaceChildren(
      btn("保存", "primary", async () => {
        const inputs = info.querySelectorAll("input");
        const updated = {
          ...site,
          title: inputs[0].value.trim(),
          url: inputs[1].value.trim(),
        };
        try {
          await invoke("update_site", { key: site.key, site: updated });
        } catch (e) {
          showError(String(e));
        }
      }),
      btn("取消", "", () => render()),
    );
  }

  function render() {
    // 默认站点下拉
    defaultSelect.replaceChildren(
      ...cfg.sites.map((s) => {
        const opt = document.createElement("option");
        opt.value = s.key;
        opt.textContent = s.title;
        return opt;
      }),
    );
    defaultSelect.value = cfg.default_key;

    // 站点列表
    listEl.replaceChildren(
      ...cfg.sites.map((s, i) => {
        const li = document.createElement("li");
        li.className = "site-row";
        li.dataset.key = s.key;

        const idx = document.createElement("span");
        idx.className = "idx";
        idx.textContent = String(i + 1);

        const info = document.createElement("div");
        info.className = "info";
        const title = document.createElement("span");
        title.className = "title";
        title.textContent = s.title;
        const meta = document.createElement("span");
        meta.className = "meta";
        meta.textContent = `${s.key} · ${s.url}`;
        info.append(title, meta);

        const theme = document.createElement("select");
        theme.className = "theme";
        for (const t of ["dark", "light"]) {
          const opt = document.createElement("option");
          opt.value = t;
          opt.textContent = t;
          theme.append(opt);
        }
        theme.value = s.theme;
        theme.addEventListener("change", async () => {
          try {
            await invoke("update_site", { key: s.key, site: { ...s, theme: theme.value } });
          } catch (e) {
            showError(String(e));
          }
        });

        const ops = document.createElement("div");
        ops.className = "ops";
        ops.append(
          btn("↑", "", () => i > 0 && reorder(i, i - 1)),
          btn("↓", "", () => i < cfg.sites.length - 1 && reorder(i, i + 1)),
          btn("编辑", "", () => editRow(li, s)),
          btn("删除", "", async () => {
            if (!confirm(`确定删除 ${s.title}？`)) return;
            try {
              await invoke("remove_site", { key: s.key });
            } catch (e) {
              showError(String(e));
            }
          }),
        );

        li.append(idx, info, theme, ops);
        return li;
      }),
    );
  }

  // 添加表单
  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    const key = document.getElementById("f-key").value.trim();
    const url = document.getElementById("f-url").value.trim();
    const title = document.getElementById("f-title").value.trim();
    const theme = document.getElementById("f-theme").value;
    if (!key || !url || !title) {
      showError("key / url / title 均不能为空");
      return;
    }
    try {
      await invoke("add_site", { site: { key, url, title, theme } });
      form.reset();
      document.getElementById("f-theme").value = "dark";
    } catch (err) {
      showError(String(err));
    }
  });

  // 默认站点变更
  defaultSelect.addEventListener("change", async () => {
    try {
      await invoke("set_default", { key: defaultSelect.value });
    } catch (e) {
      showError(String(e));
    }
  });

  async function refresh() {
    cfg = await fetchSites();
    render();
  }

  onSitesChanged((cfg2) => {
    cfg = cfg2;
    render();
  });

  refresh();
}
