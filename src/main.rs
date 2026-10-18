mod gpu;
mod mesh;
mod nbt;
mod renderer;

use std::sync::Arc;

use ash::vk;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

use renderer::device::{Swapchain, VulkanDevice};
use renderer::instance::MyInstance;
use renderer::pipeline::Pipeline;
use renderer::surface::Surface;

struct App {
    window: Option<Arc<Window>>,
    instance: Option<MyInstance>,
    surface: Option<Surface>,
    device: Option<VulkanDevice>,
    swapchain: Option<Swapchain>,
    pipeline: Option<Pipeline>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            window: None,
            instance: None,
            surface: None,
            device: None,
            swapchain: None,
            pipeline: None,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window = Arc::new(
            event_loop
                .create_window(
                    WindowAttributes::default()
                        .with_title("aseith")
                        .with_inner_size(winit::dpi::LogicalSize::new(1280, 720)),
                )
                .unwrap(),
        );

        let instance = MyInstance::new(c"aseith");
        let surface = Surface::new(&instance.inner, &instance.entry, &window);
        let device = VulkanDevice::new(&instance.entry, &instance.inner, surface.inner);

        let size = window.inner_size();
        let swapchain = Swapchain::new(
            &instance.inner,
            &device,
            surface.inner,
            size.width as u32,
            size.height as u32,
        );

        let pipeline = Pipeline::new(
            &device.logical,
            device.queue_family,
            swapchain.format,
            swapchain.extent,
            &swapchain.views,
        );

        self.window = Some(window);
        self.instance = Some(instance);
        self.surface = Some(surface);
        self.device = Some(device);
        self.swapchain = Some(swapchain);
        self.pipeline = Some(pipeline);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                let window = self.window.as_ref().unwrap();
                let device = self.device.as_ref().unwrap();
                let swapchain = self.swapchain.as_ref().unwrap();
                let pipeline = self.pipeline.as_ref().unwrap();

                unsafe {
                    device.logical.wait_for_fences(&[pipeline.in_flight], true, u64::MAX).unwrap();
                    device.logical.reset_fences(&[pipeline.in_flight]).unwrap();

                    let (index, _) = swapchain
                        .loader
                        .acquire_next_image(
                            swapchain.inner,
                            u64::MAX,
                            pipeline.image_available,
                            vk::Fence::null(),
                        )
                        .unwrap();

                    let wait_semaphores = [pipeline.image_available];
                    let wait_dst_stage_mask = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
                    let command_buffers = [pipeline.command_buffers[index as usize]];
                    let signal_semaphores = [pipeline.render_finished[index as usize]];

                    let submit = vk::SubmitInfo::default()
                        .wait_semaphores(&wait_semaphores)
                        .wait_dst_stage_mask(&wait_dst_stage_mask)
                        .command_buffers(&command_buffers)
                        .signal_semaphores(&signal_semaphores);

                    device.logical
                        .queue_submit(device.queue, &[submit], pipeline.in_flight)
                        .unwrap();

                    let signal = [pipeline.render_finished[index as usize]];
                    let chains = [swapchain.inner];
                    let indices = [index];

                    let present = vk::PresentInfoKHR::default()
                        .wait_semaphores(&signal)
                        .swapchains(&chains)
                        .image_indices(&indices);

                    swapchain.loader.queue_present(device.queue, &present).unwrap();
                }

                window.request_redraw();
            }
            _ => {}
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        unsafe {
            if let Some(device) = self.device.take() {
                device.logical.device_wait_idle().unwrap();

                if let Some(pipeline) = self.pipeline.take() {
                    pipeline.destroy(&device.logical);
                }
                if let Some(swapchain) = self.swapchain.take() {
                    swapchain.destroy(&device.logical);
                }

                device.logical.destroy_device(None);

                if let Some(surface) = self.surface.take() {
                    device.surface_loader.destroy_surface(surface.inner, None);
                }
            }
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
