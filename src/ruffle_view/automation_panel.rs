use gpui::{
    Anchor, ClickEvent, Context, DispatchPhase, Edges, Entity, Hitbox, HitboxBehavior,
    InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement as _, Pixels, Point, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, Toggled, Window, anchored, canvas, div, point,
    prelude::FluentBuilder as _, px, rgb,
};

use super::RuffleView;
use crate::automation::{
    auth::auto_relogin::{Phase, Settings},
    combat::auto_attack,
    world::auto_zone::{self, Room},
};

const DELAY_STEP: u32 = 1;
const RETRY_STEP: u32 = 1;
const BACKOFF_STEP: u32 = 1;
const BACKOFF_CAP_STEP: u32 = 5;
const WIDTH: f32 = 300.0;
const MARGIN: Pixels = px(8.0);

#[derive(Default)]
pub(super) struct Placement {
    position: Option<Point<Pixels>>,
    grab: Option<Point<Pixels>>,
}

pub(super) fn panel(
    settings: &Settings,
    phase: &Phase,
    servers: &[String],
    zone: auto_zone::Settings,
    attack: auto_attack::Settings,
    placement: &Placement,
    window: &Window,
    cx: &mut Context<RuffleView>,
) -> impl IntoElement {
    let toggle = switch("auto-relogin-toggle", "Auto relogin", settings.enabled).on_click(
        cx.listener(|this, _: &ClickEvent, _, cx| {
            this.change_auto_relogin(|settings| settings.enabled = !settings.enabled, cx)
        }),
    );
    let zone_toggle = switch("auto-zone-toggle", "Auto zone", zone.enabled).on_click(cx.listener(
        |this, _: &ClickEvent, _, cx| {
            this.change_auto_zone(|settings| settings.enabled = !settings.enabled, cx)
        },
    ));
    let attack_toggle = switch("auto-attack-toggle", "Auto attack", attack.enabled).on_click(
        cx.listener(|this, _: &ClickEvent, _, cx| {
            this.change_auto_attack(|settings| settings.enabled = !settings.enabled, cx)
        }),
    );
    let view = cx.entity();
    let header = div()
        .relative()
        .flex()
        .justify_between()
        .items_center()
        .px_3()
        .pt_3()
        .pb_1()
        .cursor_move()
        .child(
            canvas(
                |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::BlockMouse),
                move |_, hitbox, window, _| drag(view.clone(), hitbox, window),
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        )
        .child(div().text_size(px(13.0)).child("Automation"))
        .child(
            div()
                .id("automation-close")
                .role(Role::Button)
                .aria_label("Close")
                .occlude()
                .px_1()
                .rounded_sm()
                .cursor_pointer()
                .hover(|style| style.bg(rgb(0x444444)))
                .child("×")
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close_automation(cx))),
        );
    let body = div()
        .flex()
        .flex_col()
        .gap_2()
        .px_3()
        .pb_3()
        .child(row("Auto relogin").child(toggle))
        .child(server_picker(settings.server.as_deref(), servers, cx))
        .child(stepper(
            "delay",
            "Delay",
            format!("{} s", settings.delay_seconds),
            |settings| settings.delay_seconds = settings.delay_seconds.saturating_sub(DELAY_STEP),
            |settings| settings.delay_seconds += DELAY_STEP,
            cx,
        ))
        .child(stepper(
            "retries",
            "Retries",
            settings.retries.to_string(),
            |settings| settings.retries = settings.retries.saturating_sub(RETRY_STEP),
            |settings| settings.retries += RETRY_STEP,
            cx,
        ))
        .child(stepper(
            "backoff",
            "Backoff",
            format!("{} s", settings.backoff_seconds),
            |settings| {
                settings.backoff_seconds = settings.backoff_seconds.saturating_sub(BACKOFF_STEP)
            },
            |settings| settings.backoff_seconds += BACKOFF_STEP,
            cx,
        ))
        .child(stepper(
            "backoff-cap",
            "Backoff cap",
            format!("{} s", settings.backoff_cap_seconds),
            |settings| {
                settings.backoff_cap_seconds = settings
                    .backoff_cap_seconds
                    .saturating_sub(BACKOFF_CAP_STEP)
            },
            |settings| settings.backoff_cap_seconds += BACKOFF_CAP_STEP,
            cx,
        ))
        .child(div().h(px(1.0)).bg(rgb(0x3a3a3a)))
        .child(row("Auto zone").child(zone_toggle))
        .child(room_picker(zone.room, cx))
        .child(div().h(px(1.0)).bg(rgb(0x3a3a3a)))
        .child(row("Auto attack").child(attack_toggle))
        .when(*phase != Phase::Off, |panel| {
            panel.child(
                div()
                    .id("auto-relogin-status")
                    .role(Role::Status)
                    .text_color(rgb(0xb8b8b8))
                    .child(status(phase)),
            )
        });
    let panel = div()
        .id("automation-panel")
        .role(Role::Dialog)
        .aria_label("Automation")
        .occlude()
        .w(px(WIDTH))
        .flex()
        .flex_col()
        .rounded_md()
        .bg(rgb(0x1c1c1c))
        .border_1()
        .border_color(rgb(0x3a3a3a))
        .text_color(rgb(0xffffff))
        .text_size(px(12.0))
        .child(header)
        .child(body);
    let position = placement.position.unwrap_or_else(|| {
        point(
            ((window.viewport_size().width - px(WIDTH)) / 2.0).max(MARGIN),
            px(48.0),
        )
    });
    anchored()
        .anchor(Anchor::TopLeft)
        .position(position)
        .snap_to_window_with_margin(Edges::all(MARGIN))
        .child(panel)
}

fn switch(id: &'static str, label: &'static str, on: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .role(Role::Switch)
        .aria_label(label)
        .aria_toggled(if on { Toggled::True } else { Toggled::False })
        .px_2()
        .rounded_sm()
        .cursor_pointer()
        .bg(if on { rgb(0x2f6f4f) } else { rgb(0x444444) })
        .hover(|style| style.bg(rgb(0x555555)))
        .child(if on { "On" } else { "Off" })
}

fn step_server(current: Option<&str>, servers: &[String], forward: bool) -> Option<String> {
    let mut choices: Vec<Option<&str>> = vec![None];
    choices.extend(servers.iter().map(|server| Some(server.as_str())));
    if let Some(current) = current
        && !servers.iter().any(|server| server == current)
    {
        choices.push(Some(current));
    }
    let index = choices
        .iter()
        .position(|choice| *choice == current)
        .unwrap_or(0);
    let count = choices.len();
    let next = if forward {
        (index + 1) % count
    } else {
        (index + count - 1) % count
    };
    choices[next].map(str::to_owned)
}

fn server_picker(
    current: Option<&str>,
    servers: &[String],
    cx: &mut Context<RuffleView>,
) -> impl IntoElement {
    let name = current.unwrap_or("Last server").to_owned();
    let arrow = |id: &'static str, glyph: &'static str, label: &'static str, forward: bool| {
        let servers = servers.to_vec();
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(label)
            .w(px(22.0))
            .flex()
            .justify_center()
            .rounded_sm()
            .cursor_pointer()
            .bg(rgb(0x333333))
            .hover(|style| style.bg(rgb(0x555555)))
            .child(glyph)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.change_auto_relogin(
                    |settings| {
                        settings.server = step_server(settings.server.as_deref(), &servers, forward)
                    },
                    cx,
                )
            }))
    };
    row("Server").child(
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(arrow("server-previous", "‹", "Previous server", false))
            .child(
                div()
                    .id("server-value")
                    .role(Role::Label)
                    .aria_label(SharedString::from(format!("Server: {name}")))
                    .w(px(100.0))
                    .flex()
                    .justify_center()
                    .child(name),
            )
            .child(arrow("server-next", "›", "Next server", true)),
    )
}

fn step_room(room: Option<Room>, forward: bool) -> Room {
    let count = Room::ALL.len();
    let index = room.and_then(|room| Room::ALL.iter().position(|candidate| *candidate == room));
    let next = match (index, forward) {
        (None, true) => 0,
        (None, false) => count - 1,
        (Some(index), true) => (index + 1) % count,
        (Some(index), false) => (index + count - 1) % count,
    };
    Room::ALL[next]
}

fn room_picker(room: Option<Room>, cx: &mut Context<RuffleView>) -> impl IntoElement {
    let arrow = |id: &'static str, glyph: &'static str, label: &'static str, forward: bool| {
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(label)
            .w(px(22.0))
            .flex()
            .justify_center()
            .rounded_sm()
            .cursor_pointer()
            .bg(rgb(0x333333))
            .hover(|style| style.bg(rgb(0x555555)))
            .child(glyph)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.change_auto_zone(
                    |settings| settings.room = Some(step_room(settings.room, forward)),
                    cx,
                )
            }))
    };
    let name = room.map_or("Choose a room", Room::label);
    row("Room").child(
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(arrow("room-previous", "‹", "Previous room", false))
            .child(
                div()
                    .id("room-value")
                    .role(Role::Label)
                    .aria_label(SharedString::from(format!("Room: {name}")))
                    .w(px(100.0))
                    .flex()
                    .justify_center()
                    .child(name),
            )
            .child(arrow("room-next", "›", "Next room", true)),
    )
}

fn row(label: &'static str) -> gpui::Div {
    div().flex().justify_between().items_center().child(label)
}

fn stepper(
    id: &'static str,
    label: &'static str,
    value: String,
    decrease: impl Fn(&mut Settings) + 'static,
    increase: impl Fn(&mut Settings) + 'static,
    cx: &mut Context<RuffleView>,
) -> impl IntoElement {
    let button = |suffix: &'static str, glyph: &'static str, verb: &'static str| {
        div()
            .id(SharedString::from(format!("{id}-{suffix}")))
            .role(Role::Button)
            .aria_label(SharedString::from(format!(
                "{verb} {}",
                label.to_lowercase()
            )))
            .w(px(22.0))
            .flex()
            .justify_center()
            .rounded_sm()
            .cursor_pointer()
            .bg(rgb(0x333333))
            .hover(|style| style.bg(rgb(0x555555)))
            .child(glyph)
    };
    row(label).child(
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(button("decrease", "−", "Decrease").on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| this.change_auto_relogin(&decrease, cx),
            )))
            .child(
                div()
                    .id(SharedString::from(format!("{id}-value")))
                    .role(Role::Label)
                    .aria_label(SharedString::from(format!("{label}: {value}")))
                    .w(px(44.0))
                    .flex()
                    .justify_center()
                    .child(value),
            )
            .child(button("increase", "+", "Increase").on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| this.change_auto_relogin(&increase, cx),
            ))),
    )
}

fn status(phase: &Phase) -> String {
    match phase {
        Phase::Off => String::new(),
        Phase::Armed => "Watching the connection".into(),
        Phase::Waiting => "Connection lost, waiting to log in".into(),
        Phase::Attempt { attempt, of } => format!("Logging in, attempt {attempt} of {of}"),
        Phase::Backoff { attempt, of } => {
            format!("Attempt {attempt} of {of} failed, waiting to retry")
        }
        Phase::Failed(reason) => format!("Stopped: {reason}"),
    }
}

fn drag(view: Entity<RuffleView>, hitbox: Hitbox, window: &mut Window) {
    let down = view.clone();
    let handle = hitbox.clone();
    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
        if phase == DispatchPhase::Capture
            && event.button == MouseButton::Left
            && handle.is_hovered(window)
        {
            down.update(cx, |this, cx| {
                this.automation_placement.grab = Some(event.position - handle.origin);
                window.capture_pointer(handle.id);
                cx.stop_propagation();
            });
        }
    });
    let moving = view.clone();
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
        if phase != DispatchPhase::Capture {
            return;
        }
        moving.update(cx, |this, cx| {
            let placement = &mut this.automation_placement;
            let Some(grab) = placement.grab else {
                return;
            };
            if event.pressed_button == Some(MouseButton::Left) {
                let viewport = window.viewport_size();
                let origin = event.position - grab;
                placement.position = Some(point(
                    origin.x.clamp(
                        MARGIN,
                        (viewport.width - hitbox.bounds.size.width - MARGIN).max(MARGIN),
                    ),
                    origin.y.clamp(
                        MARGIN,
                        (viewport.height - hitbox.bounds.size.height - MARGIN).max(MARGIN),
                    ),
                ));
                cx.stop_propagation();
                cx.notify();
            } else {
                placement.grab = None;
                window.release_pointer();
            }
        });
    });
    window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
        if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
            view.update(cx, |this, cx| {
                if this.automation_placement.grab.take().is_some() {
                    cx.stop_propagation();
                }
            });
        }
    });
}
