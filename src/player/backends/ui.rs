use ruffle_core::backend::ui::{
    DialogResultFuture, FileFilter, FontDefinition, FullscreenError, LanguageIdentifier,
    MouseCursor, MultiDialogResultFuture, NullUiBackend, UiBackend,
};
use ruffle_core::font::FontQuery;
use url::Url;

use crate::player::Output;

pub(crate) struct Ui {
    clipboard: arboard::Clipboard,
    outputs: async_channel::Sender<Output>,
    rest: NullUiBackend,
}

impl Ui {
    pub(crate) fn new(clipboard: arboard::Clipboard, outputs: async_channel::Sender<Output>) -> Ui {
        Ui {
            clipboard,
            outputs,
            rest: NullUiBackend::new(),
        }
    }
}

impl UiBackend for Ui {
    fn clipboard_content(&mut self) -> String {
        self.clipboard.get_text().unwrap_or_default()
    }

    fn set_clipboard_content(&mut self, content: String) {
        let _ = self.clipboard.set_text(content);
    }

    fn mouse_visible(&self) -> bool {
        self.rest.mouse_visible()
    }

    fn set_mouse_visible(&mut self, visible: bool) {
        self.rest.set_mouse_visible(visible);
    }

    fn set_mouse_cursor(&mut self, cursor: MouseCursor) {
        let _ = self.outputs.try_send(Output::Cursor(cursor));
    }

    fn set_fullscreen(&mut self, is_full: bool) -> Result<(), FullscreenError> {
        self.rest.set_fullscreen(is_full)
    }

    fn display_root_movie_download_failed_message(&self, invalid_swf: bool, fetch_error: String) {
        self.rest
            .display_root_movie_download_failed_message(invalid_swf, fetch_error);
    }

    fn message(&self, message: &str) {
        self.rest.message(message);
    }

    fn open_virtual_keyboard(&self) {
        self.rest.open_virtual_keyboard();
    }

    fn close_virtual_keyboard(&self) {
        self.rest.close_virtual_keyboard();
    }

    fn language(&self) -> LanguageIdentifier {
        self.rest.language()
    }

    fn display_unsupported_video(&self, url: Url) {
        self.rest.display_unsupported_video(url);
    }

    fn load_device_font(&self, query: &FontQuery, register: &mut dyn FnMut(FontDefinition)) {
        self.rest.load_device_font(query, register);
    }

    fn sort_device_fonts(
        &self,
        query: &FontQuery,
        register: &mut dyn FnMut(FontDefinition),
    ) -> Vec<FontQuery> {
        self.rest.sort_device_fonts(query, register)
    }

    fn display_file_open_dialog(&mut self, filters: Vec<FileFilter>) -> Option<DialogResultFuture> {
        self.rest.display_file_open_dialog(filters)
    }

    fn display_file_open_dialog_multiple(
        &mut self,
        filters: Vec<FileFilter>,
    ) -> Option<MultiDialogResultFuture> {
        self.rest.display_file_open_dialog_multiple(filters)
    }

    fn display_file_save_dialog(
        &mut self,
        file_name: String,
        title: String,
    ) -> Option<DialogResultFuture> {
        self.rest.display_file_save_dialog(file_name, title)
    }

    fn close_file_dialog(&mut self) {
        self.rest.close_file_dialog();
    }
}
