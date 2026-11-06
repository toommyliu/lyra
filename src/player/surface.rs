use std::{ffi::c_void, ptr::NonNull};

use objc2::{rc::Retained, runtime::AnyObject};
use ruffle_render::backend::RenderBackend;
use ruffle_render_wgpu::{backend::WgpuRenderBackend, wgpu};

use super::view::Viewport;

pub(crate) struct Surface(Retained<AnyObject>);

// wgpu accepts a retained CAMetalLayer on the player thread.
unsafe impl Send for Surface {}

impl Surface {
    pub(crate) unsafe fn from_metal_layer(layer: NonNull<c_void>) -> Self {
        Self(unsafe { Retained::retain(layer.cast().as_ptr()) }.unwrap())
    }

    pub(crate) fn renderer(&self, viewport: Viewport) -> anyhow::Result<Box<dyn RenderBackend>> {
        let target = wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(
            Retained::as_ptr(&self.0).cast_mut().cast(),
        );
        let renderer = unsafe {
            WgpuRenderBackend::for_window_unsafe(
                target,
                (viewport.width(), viewport.height()),
                wgpu::Backends::METAL,
                wgpu::PowerPreference::LowPower,
                None,
            )
        }
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        Ok(Box::new(renderer))
    }
}
