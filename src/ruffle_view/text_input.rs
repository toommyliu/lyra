use std::ops::Range;

use gpui::{App, Bounds, Entity, InputHandler, Pixels, UTF16Selection, Window};

use crate::ruffle::view::{Ime, ViewEvent};

use super::{RuffleView, input};

pub(super) struct TextInput {
    view: Entity<RuffleView>,
    bounds: Bounds<Pixels>,
}

impl TextInput {
    pub(super) fn new(bounds: Bounds<Pixels>, view: Entity<RuffleView>) -> Self {
        Self { view, bounds }
    }
}

// GPUI's ElementInputHandler does not expose the press-and-hold policy.
impl InputHandler for TextInput {
    fn apple_press_and_hold_enabled(&mut self) -> bool {
        false
    }

    fn prefers_ime_for_printable_keys(&mut self, _window: &mut Window, _cx: &mut App) -> bool {
        true
    }

    fn text_for_range(
        &mut self,
        _range: Range<usize>,
        _adjusted: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<String> {
        None
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<UTF16Selection> {
        let end = self
            .view
            .read(cx)
            .marked
            .as_ref()
            .map_or(0, |range| range.end);
        Some(UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }

    fn marked_text_range(&mut self, _window: &mut Window, cx: &mut App) -> Option<Range<usize>> {
        self.view.read(cx).marked.clone()
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut App) {
        self.view.update(cx, |view, _| {
            view.marked = None;
            view.session.send(ViewEvent::Ime(Ime::Preedit {
                text: String::new(),
                cursor: None,
            }));
        });
    }

    fn replace_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut App,
    ) {
        self.view.update(cx, |view, _| {
            let event = match view.marked.take() {
                Some(_) => ViewEvent::Ime(Ime::Commit(text.to_owned())),
                None => ViewEvent::Text(text.to_owned()),
            };
            view.session.send(event);
        });
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut App,
    ) {
        self.view.update(cx, |view, _| {
            view.marked = (!new_text.is_empty()).then(|| 0..new_text.encode_utf16().count());
            view.session.send(ViewEvent::Ime(Ime::Preedit {
                text: new_text.to_owned(),
                cursor: new_selected_range.map(|range| {
                    input::utf16_to_byte(new_text, range.start)
                        ..input::utf16_to_byte(new_text, range.end)
                }),
            }));
        });
    }

    fn bounds_for_range(
        &mut self,
        _range: Range<usize>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        Some(Bounds::new(
            gpui::point(self.bounds.left(), self.bounds.bottom()),
            gpui::size(gpui::px(1.0), gpui::px(1.0)),
        ))
    }

    fn element_bounds(&mut self, _window: &mut Window, _cx: &mut App) -> Option<Bounds<Pixels>> {
        Some(self.bounds)
    }

    fn character_index_for_point(
        &mut self,
        _point: gpui::Point<Pixels>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<usize> {
        None
    }
}
