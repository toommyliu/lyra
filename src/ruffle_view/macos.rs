use std::ptr::NonNull;

use crate::ruffle::Surface;
use gpui::{Bounds, Pixels, Window};
use objc2::{
    AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{NSObject, NSObjectProtocol},
    sel,
};
use objc2_app_kit::{NSView, NSWindowOrderingMode};
use objc2_core_graphics::CGColor;
use objc2_foundation::{NSPoint, NSRect, NSRunLoop, NSRunLoopCommonModes, NSSize};
use objc2_quartz_core::{CADisplayLink, CAMetalLayer, CATransaction};
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
    input_view: Retained<NSView>,
    layer: Retained<CAMetalLayer>,
    frame_clock: Option<Retained<CADisplayLink>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "LyraFrameClockTarget"]
    #[ivars = Box<dyn Fn()>]
    struct FrameClockTarget;

    impl FrameClockTarget {
        #[unsafe(method(frame:))]
        fn frame(&self, _clock: &CADisplayLink) {
            (self.ivars())();
        }
    }
);

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
        let input_view =
            unsafe { Retained::retain(handle.ns_view.cast::<NSView>().as_ptr()) }.unwrap();
        Ok(NativeView {
            view,
            input_view,
            layer,
            frame_clock: None,
        })
    }

    pub(super) fn supports_display_frames(&self) -> bool {
        self.view
            .respondsToSelector(sel!(displayLinkWithTarget:selector:))
    }

    pub(super) fn connect_frames(&mut self, on_frame: impl Fn() + 'static) {
        if !self.supports_display_frames() {
            return;
        }
        let target: Retained<FrameClockTarget> = unsafe {
            msg_send![
                super(FrameClockTarget::alloc().set_ivars(Box::new(on_frame))),
                init
            ]
        };
        let clock = unsafe {
            self.view
                .displayLinkWithTarget_selector(&target, sel!(frame:))
        };
        clock.setPaused(true);
        unsafe {
            clock.addToRunLoop_forMode(&NSRunLoop::mainRunLoop(), NSRunLoopCommonModes);
        }
        self.frame_clock = Some(clock);
    }

    pub(super) fn set_frame_clock(&self, enabled: bool) {
        if let Some(clock) = &self.frame_clock {
            clock.setPaused(!enabled);
        }
    }

    pub(super) fn surface(&self) -> Surface {
        unsafe { Surface::from_metal_layer(NonNull::from(&*self.layer).cast()) }
    }

    pub(super) fn menu_views(&self) -> (Retained<NSView>, Retained<NSView>) {
        (self.view.clone(), self.input_view.clone())
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
        if let Some(clock) = &self.frame_clock {
            clock.invalidate();
        }
        self.view.removeFromSuperview();
    }
}
