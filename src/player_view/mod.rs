mod input;

#[path = "macos.rs"]
mod native;

use std::ops::Range;

use crate::player::{
    Player,
    view::{Ime, ViewEvent},
};
use gpui::{
    App, AppContext as _, Bounds, Context, DispatchPhase, ElementInputHandler, Entity,
    EntityInputHandler, FocusHandle, Focusable, Hitbox, HitboxBehavior, InteractiveElement as _,
    IntoElement, KeyDownEvent, KeyUpEvent, ModifiersChangedEvent, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement as _, Pixels, Render, ScrollWheelEvent, Styled as _, Subscription,
    UTF16Selection, Window, canvas, div, prelude::FluentBuilder as _,
};
use native::NativeView;

pub struct PlayerView {
    session: Player,
    error: Option<String>,
    native: NativeView,
    focus: FocusHandle,
    hovered: bool,
    marked: Option<Range<usize>>,
    modifiers: gpui::Modifiers,
    _subscriptions: [Subscription; 3],
}

impl PlayerView {
    pub fn new(
        data: std::path::PathBuf,
        device: reqwest::header::HeaderValue,
        window: &mut Window,
        cx: &mut App,
    ) -> anyhow::Result<Entity<PlayerView>> {
        let native = NativeView::attach(window)?;
        let (failed, failures) = async_channel::bounded(1);
        let session = Player::spawn(native.surface(), data, device, failed)?;
        Ok(cx.new(|cx| {
            cx.spawn(async move |this, cx| {
                if let Ok(error) = failures.recv().await {
                    let _ = this.update(cx, |this: &mut Self, cx| {
                        this.error = Some(error);
                        cx.notify();
                    });
                }
            })
            .detach();
            let focus = cx.focus_handle();
            let subscriptions = [
                cx.on_focus(&focus, window, |this: &mut Self, window, _| {
                    this.session
                        .send(ViewEvent::Focus(window.is_window_active()))
                }),
                cx.on_blur(&focus, window, |this: &mut Self, _, _| {
                    this.session.send(ViewEvent::Focus(false))
                }),
                cx.observe_window_activation(window, |this: &mut Self, window, _| {
                    this.session.send(ViewEvent::Focus(
                        window.is_window_active() && this.focus.is_focused(window),
                    ));
                    if !window.is_window_active() {
                        this.modifiers = Default::default();
                    }
                }),
            ];
            Self {
                session,
                error: None,
                native,
                focus,
                hovered: false,
                marked: None,
                modifiers: Default::default(),
                _subscriptions: subscriptions,
            }
        }))
    }

    fn layout(&mut self, bounds: Bounds<Pixels>, window: &Window) {
        let scale = window.scale_factor();
        self.native.set_frame(bounds, scale);
        if let Some(viewport) = input::viewport(bounds, scale) {
            self.session.send(ViewEvent::Resized(viewport));
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(key) = input::key(&event.keystroke) {
            self.session.send(ViewEvent::KeyDown(key));
        }
    }

    fn key_up(&mut self, event: &KeyUpEvent, _window: &mut Window, _cx: &mut Context<Self>) {
        if let Some(key) = input::key(&event.keystroke) {
            self.session.send(ViewEvent::KeyUp(key));
        }
    }

    fn modifiers_changed(
        &mut self,
        event: &ModifiersChangedEvent,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        for event in input::modifier_keys(self.modifiers, event.modifiers) {
            self.session.send(event);
        }
        self.modifiers = event.modifiers;
    }
}

fn paint(this: Entity<PlayerView>, hitbox: Hitbox, window: &mut Window, cx: &mut App) {
    window.handle_input(
        &this.read(cx).focus,
        ElementInputHandler::new(hitbox.bounds, this.clone()),
        cx,
    );
    let down_view = this.clone();
    let down_hitbox = hitbox.clone();
    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
        if phase == DispatchPhase::Bubble && down_hitbox.is_hovered(window) {
            window.capture_pointer(down_hitbox.id);
            down_view.update(cx, |this, cx| {
                window.focus(&this.focus, cx);
                this.session.send(ViewEvent::MouseDown {
                    at: input::point(event.position, down_hitbox.origin),
                    button: input::button(event.button),
                    clicks: event.click_count,
                });
            });
        }
    });
    let up_view = this.clone();
    let up_hitbox = hitbox.clone();
    window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
        if phase == DispatchPhase::Bubble
            && (up_hitbox.is_hovered(window) || window.captured_hitbox() == Some(up_hitbox.id))
        {
            up_view.update(cx, |this, _| {
                this.session.send(ViewEvent::MouseUp {
                    at: input::point(event.position, up_hitbox.origin),
                    button: input::button(event.button),
                })
            });
        }
    });
    let move_view = this.clone();
    let move_hitbox = hitbox.clone();
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
        if phase != DispatchPhase::Bubble {
            return;
        }
        let hovered = move_hitbox.is_hovered(window);
        let captured = window.captured_hitbox() == Some(move_hitbox.id);
        move_view.update(cx, |this, _| {
            if hovered || captured {
                this.session.send(ViewEvent::MouseMove(input::point(
                    event.position,
                    move_hitbox.origin,
                )));
            } else if this.hovered {
                this.session.send(ViewEvent::MouseLeave);
            }
            this.hovered = hovered || captured;
        });
    });
    window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
        if phase == DispatchPhase::Bubble && hitbox.should_handle_scroll(window) {
            this.update(cx, |this, _| {
                this.session
                    .send(ViewEvent::Wheel(input::wheel(event.delta)))
            });
        }
    });
}

impl Render for PlayerView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.entity();
        let prepaint = {
            let this = this.clone();
            move |bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App| {
                this.update(cx, |this, _| this.layout(bounds, window));
                window.insert_hitbox(bounds, HitboxBehavior::Normal)
            }
        };
        div()
            .size_full()
            .when_some(self.error.clone(), |view, error| {
                view.bg(gpui::rgb(0x181818))
                    .text_color(gpui::rgb(0xffffff))
                    .child(format!("Couldn't start AQW: {error}"))
            })
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::key_down))
            .on_key_up(cx.listener(Self::key_up))
            .on_modifiers_changed(cx.listener(Self::modifiers_changed))
            .child(
                canvas(prepaint, move |_bounds, hitbox, window, cx| {
                    paint(this, hitbox, window, cx)
                })
                .size_full(),
            )
    }
}

impl Focusable for PlayerView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl EntityInputHandler for PlayerView {
    fn text_for_range(
        &mut self,
        _range: Range<usize>,
        _adjusted: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        None
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let end = self.marked.as_ref().map_or(0, |range| range.end);
        Some(UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked.clone()
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked = None;
        self.session.send(ViewEvent::Ime(Ime::Preedit {
            text: String::new(),
            cursor: None,
        }));
    }

    fn replace_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        let event = match self.marked.take() {
            Some(_) => ViewEvent::Ime(Ime::Commit(text.to_owned())),
            None => ViewEvent::Text(text.to_owned()),
        };
        self.session.send(event);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.marked = (!new_text.is_empty()).then(|| 0..new_text.encode_utf16().count());
        self.session.send(ViewEvent::Ime(Ime::Preedit {
            text: new_text.to_owned(),
            cursor: new_selected_range.map(|range| {
                input::utf16_to_byte(new_text, range.start)
                    ..input::utf16_to_byte(new_text, range.end)
            }),
        }));
    }

    fn bounds_for_range(
        &mut self,
        _range: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        Some(Bounds::new(
            gpui::point(element_bounds.left(), element_bounds.bottom()),
            gpui::size(gpui::px(1.0), gpui::px(1.0)),
        ))
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}
