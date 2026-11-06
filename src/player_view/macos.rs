use std::ptr::NonNull;

use crate::player::Surface;
use gpui::{Bounds, Pixels, Window};
use objc2::{MainThreadMarker, MainThreadOnly, rc::Retained};
use objc2_app_kit::{NSView, NSWindowOrderingMode};
use objc2_core_graphics::CGColor;
use objc2_foundation::{NSPoint, NSRect, NSSize};
use objc2_quartz_core::{CAMetalLayer, CATransaction};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the window has no AppKit view")]
    NoAppKitView,
    #[error("GPUI's view isn't in a window's content view")]
    NoContentView,
}

pub(super) struct NativeView {
    view: Retained<NSView>,
    layer: Retained<CAMetalLayer>,
}

impl NativeView {
    pub(super) fn attach(window: &Window) -> Result<NativeView, Error> {
        let handle = HasWindowHandle::window_handle(window).map_err(|_| Error::NoAppKitView)?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return Err(Error::NoAppKitView);
        };
        let gpui_view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        let content = unsafe { gpui_view.superview() }.ok_or(Error::NoContentView)?;
        let mtm = MainThreadMarker::new().unwrap();
        let layer = CAMetalLayer::new();
        layer.setBackgroundColor(Some(&CGColor::new_srgb(0.0, 0.0, 0.0, 1.0)));
        layer.setOpaque(true);
        let view = NSView::initWithFrame(NSView::alloc(mtm), NSRect::ZERO);
        view.setLayer(Some(&layer));
        view.setWantsLayer(true);
        content.addSubview_positioned_relativeTo(&view, NSWindowOrderingMode::Below, None);
        Ok(NativeView { view, layer })
    }

    pub(super) fn surface(&self) -> Surface {
        unsafe { Surface::from_metal_layer(NonNull::from(&*self.layer).cast()) }
    }

    pub(super) fn set_frame(&self, bounds: Bounds<Pixels>, scale: f32) {
        let Some(content) = (unsafe { self.view.superview() }) else {
            return;
        };
        let width = f64::from(f32::from(bounds.size.width));
        let height = f64::from(f32::from(bounds.size.height));
        let x = f64::from(f32::from(bounds.origin.x));
        let y = content.bounds().size.height - f64::from(f32::from(bounds.origin.y)) - height;
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        self.view
            .setFrame(NSRect::new(NSPoint::new(x, y), NSSize::new(width, height)));
        self.layer
            .setFrame(NSRect::new(NSPoint::ZERO, NSSize::new(width, height)));
        self.layer.setContentsScale(scale.into());
        CATransaction::commit();
    }
}

impl Drop for NativeView {
    fn drop(&mut self) {
        self.view.removeFromSuperview();
    }
}
