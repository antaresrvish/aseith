use ash::{vk, Device, Entry, Instance};

pub struct Swapchain {
    pub loader: ash::khr::swapchain::Device,
    pub inner: vk::SwapchainKHR,
    pub format: vk::Format,
    pub extent: vk::Extent2D,
    pub images: Vec<vk::Image>,
    pub views: Vec<vk::ImageView>,
}

pub struct VulkanDevice {
    pub physical: vk::PhysicalDevice,
    pub logical: Device,
    pub queue_family: u32,
    pub queue: vk::Queue,
    pub surface_loader: ash::khr::surface::Instance,
}

impl VulkanDevice {
    pub fn new(entry: &Entry, instance: &Instance, surface: vk::SurfaceKHR) -> Self {
        let surface_loader = ash::khr::surface::Instance::new(entry, instance);

        unsafe {
            let devices = instance.enumerate_physical_devices().unwrap();
            let mut chosen: Option<(vk::PhysicalDevice, u32)> = None;

            for pdev in &devices {
                let families = instance.get_physical_device_queue_family_properties(*pdev);
                for (i, props) in families.iter().enumerate() {
                    if props.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                        && surface_loader
                            .get_physical_device_surface_support(*pdev, i as u32, surface)
                            .unwrap()
                    {
                        chosen = Some((*pdev, i as u32));
                        break;
                    }
                }
                if chosen.is_some() {
                    break;
                }
            }

            let (physical, queue_family) = match chosen {
                Some(v) => v,
                None => panic!("no suitable gpu"),
            };

            let mut extensions: Vec<*const i8> = vec![ash::khr::swapchain::NAME.as_ptr()];
            #[cfg(target_os = "macos")]
            {
                extensions.push(ash::khr::portability_subset::NAME.as_ptr());
            }

            let priorities = [1.0f32];
            let queue_infos = vec![vk::DeviceQueueCreateInfo::default()
                .queue_family_index(queue_family)
                .queue_priorities(&priorities)];

            let device_info = vk::DeviceCreateInfo::default()
                .queue_create_infos(&queue_infos)
                .enabled_extension_names(&extensions);

            let logical = instance.create_device(physical, &device_info, None).unwrap();
            let queue = logical.get_device_queue(queue_family, 0);

            VulkanDevice {
                physical,
                logical,
                queue_family,
                queue,
                surface_loader,
            }
        }
    }
}

impl Swapchain {
    pub fn new(
        instance: &Instance,
        device: &VulkanDevice,
        surface: vk::SurfaceKHR,
        width: u32,
        height: u32,
    ) -> Self {
        let loader = ash::khr::swapchain::Device::new(instance, &device.logical);

        unsafe {
            let caps = device.surface_loader.get_physical_device_surface_capabilities(
                device.physical,
                surface,
            ).unwrap();
            let formats = device.surface_loader.get_physical_device_surface_formats(
                device.physical,
                surface,
            ).unwrap();
            let _present_modes = device.surface_loader.get_physical_device_surface_present_modes(
                device.physical,
                surface,
            ).unwrap();

            let format = formats
                .iter()
                .find(|f| {
                    f.format == vk::Format::B8G8R8A8_SRGB
                        && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
                })
                .copied()
                .unwrap_or(formats[0]);

            let present_mode = vk::PresentModeKHR::FIFO;

            let extent = if caps.current_extent.width != u32::MAX {
                caps.current_extent
            } else {
                let w = width.clamp(caps.min_image_extent.width, caps.max_image_extent.width);
                let h = height.clamp(caps.min_image_extent.height, caps.max_image_extent.height);
                vk::Extent2D { width: w, height: h }
            };

            let mut image_count = caps.min_image_count + 1;
            if caps.max_image_count != 0 {
                image_count = image_count.min(caps.max_image_count);
            }

            let info = vk::SwapchainCreateInfoKHR::default()
                .surface(surface)
                .min_image_count(image_count)
                .image_format(format.format)
                .image_color_space(format.color_space)
                .image_extent(extent)
                .image_array_layers(1)
                .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
                .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                .pre_transform(caps.current_transform)
                .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
                .present_mode(present_mode)
                .clipped(true);

            let inner = loader.create_swapchain(&info, None).unwrap();
            let images = loader.get_swapchain_images(inner).unwrap();

            let views = images
                .iter()
                .map(|&img| {
                    let view_info = vk::ImageViewCreateInfo::default()
                        .image(img)
                        .view_type(vk::ImageViewType::TYPE_2D)
                        .format(format.format)
                        .subresource_range(
                            vk::ImageSubresourceRange::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .base_mip_level(0)
                                .level_count(1)
                                .base_array_layer(0)
                                .layer_count(1),
                        );
                    device.logical.create_image_view(&view_info, None).unwrap()
                })
                .collect();

            Swapchain {
                loader,
                inner,
                format: format.format,
                extent,
                images,
                views,
            }
        }
    }

    pub unsafe fn destroy(&self, device: &Device) {
        for view in &self.views {
            device.destroy_image_view(*view, None);
        }
        self.loader.destroy_swapchain(self.inner, None);
    }
}
