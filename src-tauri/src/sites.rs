use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{LogicalPosition, LogicalSize, Manager};

/// 顶部标签栏高度（逻辑像素），与 macOS 标题栏高度接近，融入窗口框架。
/// 内容 webview 从该高度下方开始铺满窗口。
// macOS 用原生标题栏按钮，无 webview 标签栏，此常量仅非 macOS 平台使用。
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub const TAB_BAR_HEIGHT: f64 = 32.0;

/// 单个站点的配置（serde 可序列化，作为 sites.json 中的一项）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Site {
    /// 站点 key，作为 webview label 与 IPC 标识，只允许 [A-Za-z0-9_-]。
    pub key: String,
    /// 站点 URL（https）。
    pub url: String,
    /// 显示名称。
    pub title: String,
    /// 页面默认主题（在主题检测脚本上报前使用）。
    pub theme: String,
}

/// sites.json 的根结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SitesConfig {
    /// 配置版本，预留未来迁移。
    pub version: u32,
    /// 默认激活的站点 key。
    pub default_key: String,
    /// 全部站点，顺序即下拉/循环切换顺序。
    pub sites: Vec<Site>,
}

/// 内置默认配置：首次运行种子 + 配置文件损坏时的回退。
pub fn default_config() -> SitesConfig {
    SitesConfig {
        version: 1,
        default_key: "deepseek".to_string(),
        sites: vec![
            Site {
                key: "deepseek".to_string(),
                url: "https://chat.deepseek.com".to_string(),
                title: "DeepSeek".to_string(),
                theme: "dark".to_string(),
            },
            Site {
                key: "chatglm".to_string(),
                url: "https://chatglm.cn".to_string(),
                title: "ChatGLM".to_string(),
                theme: "dark".to_string(),
            },
            Site {
                key: "zread".to_string(),
                url: "https://zread.ai".to_string(),
                title: "Zread".to_string(),
                theme: "dark".to_string(),
            },
            Site {
                key: "qianwen".to_string(),
                url: "https://www.qianwen.com".to_string(),
                title: "Qianwen".to_string(),
                theme: "light".to_string(),
            },
        ],
    }
}

/// 校验单个站点字段，返回中文错误信息。
pub fn validate_site(site: &Site) -> Result<(), String> {
    if site.key.is_empty()
        || !site
            .key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(format!("key 只能包含字母/数字/横线/下划线: {:?}", site.key));
    }
    let url = site
        .url
        .parse::<tauri::Url>()
        .map_err(|_| format!("无效 URL: {}", site.url))?;
    if url.scheme() != "https" {
        return Err(format!("URL 必须为 https: {}", site.url));
    }
    if site.title.trim().is_empty() {
        return Err(format!("站点 {} 的 title 不能为空", site.key));
    }
    if site.theme != "dark" && site.theme != "light" {
        return Err(format!("站点 {} 的 theme 必须为 dark 或 light: {}", site.key, site.theme));
    }
    Ok(())
}

/// 校验整个配置：key 唯一、默认站点存在、各站点字段合法。
pub fn validate_config(cfg: &SitesConfig) -> Result<(), String> {
    if cfg.sites.is_empty() {
        return Err("至少需要一个站点".to_string());
    }
    for s in &cfg.sites {
        validate_site(s)?;
    }
    let mut seen = std::collections::HashSet::new();
    for s in &cfg.sites {
        if !seen.insert(&s.key) {
            return Err(format!("站点 key 重复: {}", s.key));
        }
    }
    if !cfg.sites.iter().any(|s| s.key == cfg.default_key) {
        return Err(format!("默认站点 {} 不在站点列表中", cfg.default_key));
    }
    Ok(())
}

/// 站点配置运行时状态：持有内存配置与配置文件路径。
pub struct SiteStore {
    inner: Mutex<SitesConfig>,
    path: PathBuf,
}

impl SiteStore {
    /// 从 app 配置目录加载 sites.json；首次/解析失败/校验失败时回退默认并备份。
    pub fn load(app: &tauri::AppHandle) -> Result<Self, String> {
        let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join("sites.json");
        let cfg = match std::fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<SitesConfig>(&text) {
                Ok(parsed) => match validate_config(&parsed) {
                    Ok(()) => parsed,
                    Err(e) => {
                        let _ = std::fs::copy(&path, path.with_extension("json.bak"));
                        eprintln!("[chat_app] 站点配置校验失败({e})，已回退默认配置");
                        default_config()
                    }
                },
                Err(e) => {
                    let _ = std::fs::copy(&path, path.with_extension("json.bak"));
                    eprintln!("[chat_app] 站点配置解析失败({e})，已回退默认配置");
                    default_config()
                }
            },
            Err(_) => default_config(), // 首次运行
        };
        if !path.exists() {
            let text = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
            std::fs::write(&path, text).map_err(|e| e.to_string())?;
        }
        Ok(Self {
            inner: Mutex::new(cfg),
            path,
        })
    }

    /// 测试用：直接给定配置与路径（不读盘）。
    #[cfg(test)]
    pub fn from_config(cfg: SitesConfig, path: PathBuf) -> Self {
        Self {
            inner: Mutex::new(cfg),
            path,
        }
    }

    /// 当前配置的快照（clone）。
    pub fn snapshot(&self) -> SitesConfig {
        self.inner.lock().unwrap().clone()
    }

    /// 默认激活的站点 key。
    pub fn default_key(&self) -> String {
        self.inner.lock().unwrap().default_key.clone()
    }

    /// 按 key 查站点。
    pub fn site_by_key(&self, key: &str) -> Option<Site> {
        self.inner
            .lock()
            .unwrap()
            .sites
            .iter()
            .find(|s| s.key == key)
            .cloned()
    }

    /// 校验后原子写盘，再覆盖内存，返回新快照。
    fn commit(&self, cfg: SitesConfig) -> Result<SitesConfig, String> {
        validate_config(&cfg)?;
        let tmp = self.path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())?;
        *self.inner.lock().unwrap() = cfg.clone();
        Ok(cfg)
    }

    pub fn add_site(&self, site: Site) -> Result<SitesConfig, String> {
        let mut cfg = self.snapshot();
        if cfg.sites.iter().any(|s| s.key == site.key) {
            return Err(format!("站点 key 已存在: {}", site.key));
        }
        cfg.sites.push(site);
        self.commit(cfg)
    }

    /// 更新站点；不允许修改 key（改 key = 删除 + 重新添加）。
    pub fn update_site(&self, key: &str, site: Site) -> Result<SitesConfig, String> {
        if site.key != key {
            return Err("不允许修改站点 key（如需重命名请删除后重新添加）".to_string());
        }
        let mut cfg = self.snapshot();
        let idx = cfg
            .sites
            .iter()
            .position(|s| s.key == key)
            .ok_or_else(|| format!("站点不存在: {key}"))?;
        cfg.sites[idx] = site;
        self.commit(cfg)
    }

    /// 删除站点；删除默认站点时提升剩余第一个为默认；拒绝删除最后一个。
    pub fn remove_site(&self, key: &str) -> Result<SitesConfig, String> {
        let mut cfg = self.snapshot();
        let idx = cfg
            .sites
            .iter()
            .position(|s| s.key == key)
            .ok_or_else(|| format!("站点不存在: {key}"))?;
        cfg.sites.remove(idx);
        if cfg.sites.is_empty() {
            return Err("不能删除最后一个站点".to_string());
        }
        if cfg.default_key == key {
            cfg.default_key = cfg.sites[0].key.clone();
        }
        self.commit(cfg)
    }

    /// 按给定 key 顺序重排站点（必须是当前站点 key 的全排列）。
    pub fn reorder_sites(&self, keys: Vec<String>) -> Result<SitesConfig, String> {
        let cfg = self.snapshot();
        if keys.len() != cfg.sites.len() {
            return Err("排序列表长度与站点数不符".to_string());
        }
        let mut map = std::collections::HashMap::new();
        for s in &cfg.sites {
            map.insert(s.key.clone(), s.clone());
        }
        let mut ordered = Vec::with_capacity(keys.len());
        for k in keys {
            match map.remove(&k) {
                Some(s) => ordered.push(s),
                None => return Err(format!("排序列表包含未知站点: {k}")),
            }
        }
        let mut cfg = cfg;
        cfg.sites = ordered;
        self.commit(cfg)
    }

    pub fn set_default(&self, key: &str) -> Result<SitesConfig, String> {
        let mut cfg = self.snapshot();
        if !cfg.sites.iter().any(|s| s.key == key) {
            return Err(format!("站点不存在: {key}"));
        }
        cfg.default_key = key.to_string();
        self.commit(cfg)
    }
}

/// 计算内容区 webview 的位置与尺寸：从 y_offset 之下铺满窗口。
/// - macOS 用原生标题栏按钮，无 webview 标签栏，offset 为 0；
/// - 其它平台有 webview 标签栏，offset 为 TAB_BAR_HEIGHT。
/// 窗口尺寸为负或高度不足 offset 时钳制为 0，避免非法布局。
pub fn content_bounds(y_offset: f64, win_w: f64, win_h: f64) -> (LogicalPosition<f64>, LogicalSize<f64>) {
    let w = win_w.max(0.0);
    let h = (win_h - y_offset).max(0.0);
    (LogicalPosition::new(0.0, y_offset), LogicalSize::new(w, h))
}

/// 计算顶部工具栏 webview 的位置与尺寸：占满窗口宽度、固定高度；
/// 窗口高度不足工具栏高度时高度钳制为窗口高度。
// macOS 用原生标题栏按钮，无 webview 标签栏，此函数仅非 macOS 平台使用。
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn tab_bounds(win_w: f64, win_h: f64) -> (LogicalPosition<f64>, LogicalSize<f64>) {
    let w = win_w.max(0.0);
    let h = TAB_BAR_HEIGHT.min(win_h.max(0.0));
    (LogicalPosition::new(0.0, 0.0), LogicalSize::new(w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("chat_app_{}_{name}", std::process::id()))
    }

    #[test]
    fn default_config_contains_four_valid_sites() {
        let cfg = default_config();
        assert_eq!(cfg.sites.len(), 4);
        assert_eq!(cfg.default_key, "deepseek");
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn validate_rejects_duplicate_key() {
        let mut cfg = default_config();
        cfg.sites.push(cfg.sites[0].clone());
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn validate_rejects_http_url() {
        let mut cfg = default_config();
        cfg.sites[0].url = "http://example.com".into();
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn validate_rejects_empty_title() {
        let mut cfg = default_config();
        cfg.sites[0].title = "  ".into();
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn validate_rejects_bad_theme() {
        let mut cfg = default_config();
        cfg.sites[0].theme = "blue".into();
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn validate_rejects_bad_key() {
        let mut cfg = default_config();
        cfg.sites[0].key = "bad key!".into();
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn add_site_ok_and_duplicate_rejected() {
        let store = SiteStore::from_config(default_config(), test_path("add"));
        let new = Site {
            key: "gemini".into(),
            url: "https://gemini.google.com".into(),
            title: "Gemini".into(),
            theme: "dark".into(),
        };
        let cfg = store.add_site(new.clone()).unwrap();
        assert_eq!(cfg.sites.len(), 5);
        assert_eq!(store.snapshot().sites.len(), 5);
        assert!(store.add_site(new).is_err(), "重复 key 应拒绝");
    }

    #[test]
    fn update_site_key_is_immutable() {
        let store = SiteStore::from_config(default_config(), test_path("update"));
        let mut site = store.site_by_key("deepseek").unwrap();
        site.key = "other".into();
        assert!(store.update_site("deepseek", site).is_err());
    }

    #[test]
    fn remove_site_promotes_first_as_default() {
        let store = SiteStore::from_config(default_config(), test_path("remove"));
        let cfg = store.remove_site("deepseek").unwrap();
        assert_eq!(cfg.default_key, "chatglm");
        assert!(!cfg.sites.iter().any(|s| s.key == "deepseek"));
        // 删除后只剩一个时拒绝删除
        let store2 = SiteStore::from_config(
            SitesConfig { version: 1, default_key: "a".into(), sites: vec![Site {
                key: "a".into(), url: "https://a.com".into(), title: "A".into(), theme: "dark".into(),
            }] },
            test_path("remove_last"),
        );
        assert!(store2.remove_site("a").is_err());
    }

    #[test]
    fn reorder_sites_requires_full_permutation() {
        let store = SiteStore::from_config(default_config(), test_path("reorder"));
        let cfg = store
            .reorder_sites(vec![
                "qianwen".into(),
                "zread".into(),
                "chatglm".into(),
                "deepseek".into(),
            ])
            .unwrap();
        assert_eq!(cfg.sites[0].key, "qianwen");
        assert!(store.reorder_sites(vec!["deepseek".into()]).is_err());
        assert!(store.reorder_sites(vec!["deepseek".into(), "foo".into(), "bar".into(), "baz".into()]).is_err());
    }

    #[test]
    fn set_default_ok_and_unknown_rejected() {
        let store = SiteStore::from_config(default_config(), test_path("default"));
        let cfg = store.set_default("zread").unwrap();
        assert_eq!(cfg.default_key, "zread");
        assert!(store.set_default("nope").is_err());
    }

    #[test]
    fn content_bounds_offsets_by_given_offset() {
        let (pos, size) = content_bounds(TAB_BAR_HEIGHT, 1200.0, 800.0);
        assert_eq!(pos.x, 0.0);
        assert_eq!(pos.y, TAB_BAR_HEIGHT);
        assert_eq!(size.width, 1200.0);
        assert_eq!(size.height, 800.0 - TAB_BAR_HEIGHT);
        // offset 为 0 时内容铺满全窗口（macOS 无标签栏）
        let (pos0, size0) = content_bounds(0.0, 1200.0, 800.0);
        assert_eq!(pos0.y, 0.0);
        assert_eq!(size0.height, 800.0);
    }

    #[test]
    fn content_bounds_clamps_when_window_too_small() {
        // 高度不足 offset
        let (_, size) = content_bounds(TAB_BAR_HEIGHT, 800.0, 20.0);
        assert_eq!(size.height, 0.0);
        // 负宽度
        let (_, size2) = content_bounds(TAB_BAR_HEIGHT, -10.0, 100.0);
        assert_eq!(size2.width, 0.0);
    }

    #[test]
    fn tab_bounds_full_width_top() {
        let (pos, size) = tab_bounds(1200.0, 800.0);
        assert_eq!(pos.x, 0.0);
        assert_eq!(pos.y, 0.0);
        assert_eq!(size.width, 1200.0);
        assert_eq!(size.height, TAB_BAR_HEIGHT);
    }

    #[test]
    fn tab_bounds_clamps_height_when_window_smaller_than_tabbar() {
        let (_, size) = tab_bounds(1200.0, 20.0);
        assert_eq!(size.height, 20.0);
        // 负宽度
        let (_, size2) = tab_bounds(-10.0, 100.0);
        assert_eq!(size2.width, 0.0);
    }
}
