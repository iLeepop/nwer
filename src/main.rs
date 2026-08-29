use gpui::*;
use gpui_component::{Root, *};
use nwer::app::AppState;
use nwer::ui::Workspace;

fn main() {
    let app = gpui_platform::application().with_assets(gpui_component_assets::Assets);

    app.run(move |cx| {
        // This must be called before using any GPUI Component features.
        gpui_component::init(cx);

        cx.spawn(async move |cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let state = AppState::load().unwrap_or_else(|err| {
                    eprintln!("failed to load config, using defaults: {err:#}");
                    let fallback = dirs::config_dir()
                        .unwrap_or_else(|| std::path::PathBuf::from("."))
                        .join("nwer")
                        .join("config.json");
                    AppState::load_from(fallback).expect("fallback AppState")
                });
                let view = cx.new(|cx| Workspace::new(state, cx));
                cx.new(|cx| Root::new(view, window, cx).bg(cx.theme().background))
            })
            .expect("Failed to open window");
        })
        .detach();
    });
}
