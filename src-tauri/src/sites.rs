use tauri::{LogicalPosition, LogicalSize};

/// 顶部标签栏高度（逻辑像素）。内容 webview 从该高度下方开始铺满窗口。
pub const TAB_BAR_HEIGHT: f64 = 44.0;

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

/// 计算内容区 webview 的位置与尺寸：顶部标签栏之下铺满窗口。
/// 窗口尺寸为负或高度不足标签栏时钳制为 0，避免非法布局。
pub fn content_bounds(win_w: f64, win_h: f64) -> (LogicalPosition<f64>, LogicalSize<f64>) {
    let w = win_w.max(0.0);
    let h = (win_h - TAB_BAR_HEIGHT).max(0.0);
    (LogicalPosition::new(0.0, TAB_BAR_HEIGHT), LogicalSize::new(w, h))
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
    fn content_bounds_offsets_by_tab_bar_height() {
        let (pos, size) = content_bounds(1200.0, 800.0);
        assert_eq!(pos.x, 0.0);
        assert_eq!(pos.y, TAB_BAR_HEIGHT);
        assert_eq!(size.width, 1200.0);
        assert_eq!(size.height, 800.0 - TAB_BAR_HEIGHT);
    }

    #[test]
    fn content_bounds_clamps_when_window_too_small() {
        // 高度不足标签栏
        let (_, size) = content_bounds(800.0, 20.0);
        assert_eq!(size.height, 0.0);
        // 负宽度
        let (_, size2) = content_bounds(-10.0, 100.0);
        assert_eq!(size2.width, 0.0);
    }
}
