use ash::{vk, Entry, Instance};
use std::ffi::CStr;

pub struct MyInstance {
    pub entry: Entry,
    pub inner: Instance,
}

impl MyInstance {
    pub fn new(app_name: &CStr) -> Self {
        let entry = unsafe { Entry::load().expect("failed to load vulkan") };

        let app_info = vk::ApplicationInfo::default()
            .application_name(app_name)
            .application_version(vk::make_api_version(0, 1, 0, 0))
            .engine_name(app_name)
            .engine_version(vk::make_api_version(0, 1, 0, 0))
            .api_version(vk::API_VERSION_1_3);

        let mut extension_names: Vec<*const std::os::raw::c_char> = vec![ash::khr::surface::NAME.as_ptr()];

        let mut flags = vk::InstanceCreateFlags::empty();

        #[cfg(target_os = "macos")]
        {
            extension_names.push(ash::ext::metal_surface::NAME.as_ptr());
            extension_names.push(ash::khr::portability_enumeration::NAME.as_ptr());
            flags |= vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR;
        }

        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(&extension_names)
            .flags(flags);

        let inner = unsafe { entry.create_instance(&create_info, None).expect("failed to create instance") };

        Self { entry, inner }
    }
}

impl Drop for MyInstance {
    fn drop(&mut self) {
        unsafe { self.inner.destroy_instance(None); }
    }
}
