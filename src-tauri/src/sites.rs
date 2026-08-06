use tauri::{LogicalPosition, LogicalSize};

/// 顶部标签栏高度（逻辑像素），与 macOS 标题栏高度接近，融入窗口框架。
/// 内容 webview 从该高度下方开始铺满窗口。
// macOS 用原生标题栏按钮，无 webview 标签栏，此常量仅非 macOS 平台使用。
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub const TAB_BAR_HEIGHT: f64 = 32.0;

/// 默认激活的站点 key（与前端 src/tabs.js 的 DEFAULT_TAB 保持一致）。
pub const DEFAULT_KEY: &str = "deepseek";

/// 单个站点的配置。
#[derive(Debug, PartialEq, Eq)]
pub struct Site {
    /// 标签 key，前端标签栏与 Rust IPC 使用同一套 key。
    pub key: &'static str,
    /// 外部站点 URL。
    pub url: &'static str,
    /// 切换后窗口标题。
    pub title: &'static str,
    /// 页面默认主题（在主题检测脚本上报前使用）。
    pub theme: &'static str,
}

/// 全部站点，顺序即标签栏顺序。
pub fn sites() -> &'static [Site] {
    &[
        Site {
            key: "deepseek",
            url: "https://chat.deepseek.com",
            title: "DeepSeek",
            theme: "dark",
        },
        Site {
            key: "chatglm",
            url: "https://chatglm.cn",
            title: "ChatGLM",
            theme: "dark",
        },
        Site {
            key: "zread",
            url: "https://zread.ai",
            title: "Zread",
            theme: "dark",
        },
    ]
}

/// 按 key 查找站点；未知 key 返回 None。
pub fn site_by_key(key: &str) -> Option<&'static Site> {
    sites().iter().find(|s| s.key == key)
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

/// 计算顶部标签栏 webview 的位置与尺寸：占满窗口宽度、固定高度；
/// 窗口高度不足标签栏高度时高度钳制为窗口高度。
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

    #[test]
    fn sites_contains_all_three_with_valid_fields() {
        assert_eq!(sites().len(), 3);
        for s in sites() {
            assert!(!s.key.is_empty());
            assert!(s.url.starts_with("https://"), "url must be https: {}", s.url);
            assert!(!s.title.is_empty());
            assert!(!s.theme.is_empty());
        }
    }

    #[test]
    fn site_by_key_maps_each_site() {
        assert_eq!(site_by_key("deepseek").map(|s| s.url), Some("https://chat.deepseek.com"));
        assert_eq!(site_by_key("chatglm").map(|s| s.url), Some("https://chatglm.cn"));
        assert_eq!(site_by_key("zread").map(|s| s.url), Some("https://zread.ai"));
    }

    #[test]
    fn site_by_key_unknown_or_empty_returns_none() {
        assert_eq!(site_by_key("foo"), None);
        assert_eq!(site_by_key(""), None);
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

    #[test]
    fn default_key_is_first_site() {
        assert_eq!(DEFAULT_KEY, "deepseek");
        assert_eq!(site_by_key(DEFAULT_KEY).map(|s| s.url), Some("https://chat.deepseek.com"));
    }
}
