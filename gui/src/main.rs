//! Graphical client for Flydigi Vader 4 controllers.

// `use gpui_kit::*` is how GPUI Kit is meant to be used, and views are long builder chains.
#![allow(clippy::wildcard_imports, clippy::too_many_lines)]

mod app;
mod device;
mod model;
mod pages;
mod prelude;
mod sidebar;
mod theme;
mod widgets;

use std::borrow::Cow;

use gpui_kit::component::TitleBar;
use gpui_kit::*;
use tracing::error;
use tracing_subscriber::EnvFilter;

gpui_kit::assets::icon_assets!(
    ExtraIcons,
    [
        Battery,
        BatteryCharging,
        BatteryFull,
        BatteryLow,
        BatteryMedium,
        Gamepad2,
        GripVertical,
        Pause,
        Pencil,
        Play,
        RotateCcw
    ]
);

struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

fn main() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).with_writer(std::io::stderr).init();

    gpui_kit::application().with_assets(AppAssets).run(|cx| {
        gpui_kit::init(cx);
        theme::init(cx);
        let bounds = Bounds::centered(None, size(px(1200.), px(780.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(980.), px(600.))),
            app_id: Some("flugdiggi".into()),
            ..TitleBar::window_options()
        };
        if let Err(e) = gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| app::FlugApp::new(window, cx))) {
            error!("cannot open the window: {e:#}");
            cx.quit();
        }
    });
}
