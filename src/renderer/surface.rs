use ash::vk;
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

pub struct Surface {
    pub inner: vk::SurfaceKHR,
}

impl Surface {
    pub fn new(instance: &ash::Instance, entry: &ash::Entry, window: &Window) -> Self {
        let display_handle = window.display_handle().expect("Failed to get display handle");
        let window_handle = window.window_handle().expect("Failed to get window handle");

        let inner = unsafe {
            ash_window::create_surface(
                entry,
                instance,
                display_handle.as_raw(),
                window_handle.as_raw(),
                None,
            )
            .expect("Failed to create Vulkan surface")
        };

        Self { inner }
    }
}
