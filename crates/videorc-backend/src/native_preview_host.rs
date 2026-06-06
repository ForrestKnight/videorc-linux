use crate::protocol::PreviewSurfaceBounds;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativePreviewHostBounds {
    pub screen_x: f64,
    pub screen_y: f64,
    pub width: f64,
    pub height: f64,
    pub scale_factor: f64,
}

impl NativePreviewHostBounds {
    #[allow(dead_code)]
    pub fn from_surface_bounds(bounds: &PreviewSurfaceBounds) -> Self {
        Self {
            screen_x: bounds.screen_x,
            screen_y: bounds.screen_y,
            width: bounds.width.max(1.0),
            height: bounds.height.max(1.0),
            scale_factor: bounds.scale_factor.max(1.0),
        }
    }

    pub fn drawable_size(self) -> (f64, f64) {
        (
            self.width * self.scale_factor,
            self.height * self.scale_factor,
        )
    }
}

#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod macos {
    use objc2::{ClassType, MainThreadMarker, MainThreadOnly};
    use objc2::rc::Retained;
    use objc2_app_kit::NSView;
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use objc2_quartz_core::{CALayer, CAMetalLayer};

    use super::NativePreviewHostBounds;
    use crate::metal_compositor::{make_preview_layer, MetalPreviewPresenter};

    #[derive(Debug)]
    pub struct NativePreviewLayerHost {
        view: Retained<NSView>,
        layer: Retained<CAMetalLayer>,
        bounds: NativePreviewHostBounds,
    }

    impl NativePreviewLayerHost {
        pub fn new(
            presenter: &MetalPreviewPresenter,
            bounds: NativePreviewHostBounds,
            mtm: MainThreadMarker,
        ) -> Self {
            let (drawable_width, drawable_height) = bounds.drawable_size();
            let layer = make_preview_layer(presenter.device(), drawable_width, drawable_height);
            let frame = NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(bounds.width, bounds.height),
            );
            let view = NSView::initWithFrame(NSView::alloc(mtm), frame);
            view.setWantsLayer(true);
            let ca_layer: &CALayer = layer.as_super();
            view.setLayer(Some(ca_layer));
            Self {
                view,
                layer,
                bounds,
            }
        }

        pub fn view(&self) -> &NSView {
            &self.view
        }

        pub fn layer(&self) -> &CAMetalLayer {
            &self.layer
        }

        pub fn bounds(&self) -> NativePreviewHostBounds {
            self.bounds
        }
    }
}

#[cfg(target_os = "macos")]
#[allow(unused_imports)]
pub use macos::NativePreviewLayerHost;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_bounds_clamp_to_visible_drawable_size() {
        let bounds = PreviewSurfaceBounds {
            screen_x: 10.0,
            screen_y: 20.0,
            width: 0.0,
            height: 450.0,
            scale_factor: 2.0,
        };

        let host_bounds = NativePreviewHostBounds::from_surface_bounds(&bounds);

        assert_eq!(host_bounds.width, 1.0);
        assert_eq!(host_bounds.height, 450.0);
        assert_eq!(host_bounds.drawable_size(), (2.0, 900.0));
    }
}
