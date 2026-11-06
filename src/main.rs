#[cfg(not(target_os = "macos"))]
compile_error!("Lyra supports macOS only.");

mod net;
mod player;
mod player_view;

use gpui::{
    Bounds, KeyBinding, Menu, MenuItem, TitlebarOptions, WindowBackgroundAppearance, WindowBounds,
    WindowOptions, actions, px, size,
};
use tracing_subscriber::EnvFilter;

actions!(lyra, [Quit]);

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();
    let data = dirs::data_dir()
        .ok_or_else(|| anyhow::anyhow!("No application data directory"))?
        .join("Lyra");
    std::fs::create_dir_all(&data)?;
    let device = net::device_id(&data.join("device-id"))?;
    gpui_platform::application().run(move |cx| {
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.set_menus(vec![Menu {
            name: "Lyra".into(),
            disabled: false,
            items: vec![MenuItem::action("Quit Lyra", Quit)],
        }]);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(960.0), px(550.0)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("Lyra".into()),
                ..Default::default()
            }),
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        };
        let opened = cx.open_window(options, |window, cx| {
            let view = player_view::PlayerView::new(data, device, window, cx)
                .expect("create the AQW player view");
            window.focus(&gpui::Focusable::focus_handle(view.read(cx), cx), cx);
            view
        });
        if let Err(error) = opened {
            tracing::error!(%error, "couldn't open Lyra");
            cx.quit();
        }
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.activate(true);
    });
    Ok(())
}
