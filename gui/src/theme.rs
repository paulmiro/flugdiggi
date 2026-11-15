//! Dark theme from the mockups: neutral greys, Flydigi blue (darkened) as the accent.

use std::rc::Rc;

use gpui_kit::component::{Theme, ThemeMode, ThemeSet};
use gpui_kit::{App, Hsla, rgb};
use tracing::error;

const THEME: &str = r##"{
  "name": "Flugdiggi",
  "themes": [{
    "name": "Flugdiggi Dark",
    "mode": "dark",
    "font.size": 14,
    "radius": 6,
    "radius.lg": 10,
    "shadow": false,
    "colors": {
      "accent.background": "#3a3a3a",
      "accent.foreground": "#ececec",
      "accordion.background": "#2c2c2c",
      "background": "#1e1e1e",
      "border": "#363636",
      "group_box.background": "#2c2c2c",
      "group_box.foreground": "#ececec",
      "caret": "#ececec",
      "danger.background": "#e5484d",
      "danger.foreground": "#ffffff",
      "drag_border": "#5b80ff",
      "drop_target.background": "#2251e033",
      "foreground": "#ececec",
      "info.background": "#2251e0",
      "info.foreground": "#ffffff",
      "input.border": "#3a3a3a",
      "link.foreground": "#8aa6ff",
      "link.active.foreground": "#5b80ff",
      "link.hover.foreground": "#a8bdff",
      "list.background": "#2c2c2c",
      "list.active.background": "#3a3a3a",
      "list.active.border": "#3a3a3a",
      "list.even.background": "#2c2c2c",
      "list.head.background": "#2c2c2c",
      "list.hover.background": "#333333",
      "muted.background": "#2c2c2c",
      "muted.foreground": "#a0a0a0",
      "overlay": "#00000066",
      "popover.background": "#383838",
      "popover.foreground": "#ececec",
      "primary.background": "#2251e0",
      "primary.active.background": "#1c45c2",
      "primary.foreground": "#ffffff",
      "primary.hover.background": "#2c5cf0",
      "progress_bar.background": "#5b80ff",
      "ring": "#5b80ff",
      "scrollbar.background": "#1e1e1e00",
      "scrollbar.thumb.background": "#5a5a5ae6",
      "scrollbar.thumb.hover.background": "#6a6a6a",
      "secondary.background": "#3a3a3a",
      "secondary.active.background": "#444444",
      "secondary.foreground": "#ececec",
      "secondary.hover.background": "#4a4a4a",
      "selection.background": "#2251e0",
      "sidebar.background": "#282828",
      "sidebar.accent.background": "#3a3a3a",
      "sidebar.accent.foreground": "#ececec",
      "sidebar.border": "#333333",
      "sidebar.foreground": "#ececec",
      "sidebar.primary.background": "#2251e0",
      "sidebar.primary.foreground": "#ffffff",
      "skeleton.background": "#2c2c2c",
      "slider.bar.background": "#5b80ff",
      "slider.thumb.background": "#ececec",
      "success.background": "#30a46c",
      "success.foreground": "#ffffff",
      "switch.background": "#4a4a4a",
      "tab.background": "#00000000",
      "tab.active.background": "#3a3a3a",
      "tab.active.foreground": "#ececec",
      "tab.foreground": "#a0a0a0",
      "tab_bar.background": "#1e1e1e",
      "tab_bar.segmented.background": "#1e1e1e",
      "table.background": "#2c2c2c",
      "table.row.border": "#363636",
      "title_bar.background": "#1e1e1e",
      "title_bar.border": "#333333",
      "status_bar.background": "#1e1e1e",
      "status_bar.border": "#333333",
      "warning.background": "#f5a524",
      "warning.foreground": "#1e1e1e",
      "window.border": "#3a3a3a"
    }
  }]
}"##;

/// Colours the theme has no slot for.
pub struct Colors;

impl Colors {
    /// Background of grouped rows.
    pub fn card() -> Hsla {
        rgb(0x002c_2c2c).into()
    }
    /// Grid lines and empty LEDs.
    pub fn line() -> Hsla {
        rgb(0x0036_3636).into()
    }
    /// Unsaved-change markers and edited curves.
    pub fn edited() -> Hsla {
        rgb(0x005b_80ff).into()
    }
    pub fn highlight() -> Hsla {
        rgb(0x008a_a6ff).into()
    }
}

pub fn init(cx: &mut App) {
    match serde_json::from_str::<ThemeSet>(THEME) {
        Ok(set) => {
            if let Some(config) = set.themes.into_iter().next() {
                let config = Rc::new(config);
                Theme::update(cx, |t| t.apply_config(&config));
            }
        }
        Err(e) => error!("the built-in theme does not parse: {e}"),
    }
    Theme::change(ThemeMode::Dark, None, cx);
}
