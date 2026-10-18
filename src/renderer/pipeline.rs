use ash::{vk, Device};

pub struct Pipeline {
    pub render_pass: vk::RenderPass,
    pub layout: vk::PipelineLayout,
    pub inner: vk::Pipeline,
    pub framebuffers: Vec<vk::Framebuffer>,
    pub command_pool: vk::CommandPool,
    pub command_buffers: Vec<vk::CommandBuffer>,
    pub image_available: vk::Semaphore,
    pub render_finished: Vec<vk::Semaphore>,
    pub in_flight: vk::Fence,
}

fn to_words(bytes: &[u8]) -> Vec<u32> {
    bytes.chunks_exact(4)
        .map(|c| u32::from_ne_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

impl Pipeline {
    pub fn new(
        device: &Device,
        queue_family: u32,
        format: vk::Format,
        extent: vk::Extent2D,
        views: &[vk::ImageView],
    ) -> Self {
        unsafe {
            let attachment_description = vk::AttachmentDescription::default()
                .format(format)
                .samples(vk::SampleCountFlags::TYPE_1)
                .load_op(vk::AttachmentLoadOp::CLEAR)
                .store_op(vk::AttachmentStoreOp::STORE)
                .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
                .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);

            let attachment_references = [vk::AttachmentReference {
                attachment: 0,
                layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            }];

            let subpass = vk::SubpassDescription::default()
                .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                .color_attachments(&attachment_references);

            let subpasses = [subpass];

            let dependency = vk::SubpassDependency::default()
                .src_subpass(vk::SUBPASS_EXTERNAL)
                .dst_subpass(0)
                .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
                .src_access_mask(vk::AccessFlags::empty())
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE);

            let dependencies = [dependency];

            let attachments = [attachment_description];

            let render_pass_info = vk::RenderPassCreateInfo::default()
                .attachments(&attachments)
                .subpasses(&subpasses)
                .dependencies(&dependencies);

            let render_pass = device.create_render_pass(&render_pass_info, None).expect("create_render_pass");

            let vert_bytes = include_bytes!("../../shaders/spirv/renderer_vert.spv");
            let frag_bytes = include_bytes!("../../shaders/spirv/renderer_frag.spv");

            let vert_words = to_words(vert_bytes);
            let frag_words = to_words(frag_bytes);

            let entry_name = c"main";

            let vert_module_info = vk::ShaderModuleCreateInfo::default()
                .code(&vert_words);
            let frag_module_info = vk::ShaderModuleCreateInfo::default()
                .code(&frag_words);

            let vert_module = device.create_shader_module(&vert_module_info, None).expect("create_vert_shader");
            let frag_module = device.create_shader_module(&frag_module_info, None).expect("create_frag_shader");

            let layout_info = vk::PipelineLayoutCreateInfo::default();
            let layout = device.create_pipeline_layout(&layout_info, None).expect("create_pipeline_layout");

            let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::default();

            let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
                .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
                .primitive_restart_enable(false);

            let viewport = vk::Viewport {
                x: 0.0,
                y: 0.0,
                width: extent.width as f32,
                height: extent.height as f32,
                min_depth: 0.0,
                max_depth: 1.0,
            };
            let viewports = [viewport];

            let scissor = vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent,
            };
            let scissors = [scissor];

            let viewport_state = vk::PipelineViewportStateCreateInfo::default()
                .viewports(&viewports)
                .scissors(&scissors);

            let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
                .polygon_mode(vk::PolygonMode::FILL)
                .line_width(1.0)
                .cull_mode(vk::CullModeFlags::NONE)
                .front_face(vk::FrontFace::CLOCKWISE)
                .depth_clamp_enable(false)
                .rasterizer_discard_enable(false)
                .depth_bias_enable(false);

            let multisample = vk::PipelineMultisampleStateCreateInfo::default()
                .rasterization_samples(vk::SampleCountFlags::TYPE_1)
                .sample_shading_enable(false);

            let blend_attachment = vk::PipelineColorBlendAttachmentState::default()
                .color_write_mask(vk::ColorComponentFlags::RGBA)
                .blend_enable(false);
            let blend_attachments = [blend_attachment];

            let blend_state = vk::PipelineColorBlendStateCreateInfo::default()
                .attachments(&blend_attachments);

            let vert_stage = vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vert_module)
                .name(entry_name);
            let frag_stage = vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(frag_module)
                .name(entry_name);
            let stages = [vert_stage, frag_stage];

            let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
                .stages(&stages)
                .vertex_input_state(&vertex_input_state)
                .input_assembly_state(&input_assembly)
                .viewport_state(&viewport_state)
                .rasterization_state(&rasterization)
                .multisample_state(&multisample)
                .color_blend_state(&blend_state)
                .layout(layout)
                .render_pass(render_pass)
                .subpass(0);

            let pipelines = device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
                .map_err(|e| e.1)
                .expect("create_graphics_pipelines");

            device.destroy_shader_module(vert_module, None);
            device.destroy_shader_module(frag_module, None);

            let inner = pipelines[0];

            let mut framebuffers = Vec::with_capacity(views.len());
            for view in views {
                let attachments = [*view];
                let fb_info = vk::FramebufferCreateInfo::default()
                    .render_pass(render_pass)
                    .attachments(&attachments)
                    .width(extent.width)
                    .height(extent.height)
                    .layers(1);
                let fb = device.create_framebuffer(&fb_info, None).expect("create_framebuffer");
                framebuffers.push(fb);
            }

            let pool_info = vk::CommandPoolCreateInfo::default()
                .queue_family_index(queue_family)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
            let command_pool = device.create_command_pool(&pool_info, None).expect("create_command_pool");

            let alloc_info = vk::CommandBufferAllocateInfo::default()
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(views.len() as u32);
            let command_buffers = device.allocate_command_buffers(&alloc_info).expect("allocate_command_buffers");

            for (i, cb) in command_buffers.iter().enumerate() {
                let begin_info = vk::CommandBufferBeginInfo::default();
                device.begin_command_buffer(*cb, &begin_info).expect("begin_command_buffer");

                let clear_value = vk::ClearValue {
                    color: vk::ClearColorValue { float32: [0.01, 0.01, 0.015, 1.0] },
                };
                let clear_values = [clear_value];
                let render_area = vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent,
                };
                let rp_begin = vk::RenderPassBeginInfo::default()
                    .render_pass(render_pass)
                    .framebuffer(framebuffers[i])
                    .render_area(render_area)
                    .clear_values(&clear_values);

                device.cmd_begin_render_pass(*cb, &rp_begin, vk::SubpassContents::INLINE);
                device.cmd_bind_pipeline(*cb, vk::PipelineBindPoint::GRAPHICS, inner);
                device.cmd_draw(*cb, 3, 1, 0, 0);
                device.cmd_end_render_pass(*cb);

                device.end_command_buffer(*cb).expect("end_command_buffer");
            }

            let image_available = device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                .expect("create_image_available_semaphore");

            let mut render_finished = Vec::with_capacity(views.len());
            for _ in 0..views.len() {
                let sem = device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
                    .expect("create_render_finished_semaphore");
                render_finished.push(sem);
            }

            let fence_info = vk::FenceCreateInfo::default()
                .flags(vk::FenceCreateFlags::SIGNALED);
            let in_flight = device.create_fence(&fence_info, None).expect("create_fence");

            Pipeline {
                render_pass,
                layout,
                inner,
                framebuffers,
                command_pool,
                command_buffers,
                image_available,
                render_finished,
                in_flight,
            }
        }
    }

    pub unsafe fn destroy(&self, device: &Device) {
        device.destroy_fence(self.in_flight, None);
        for sem in &self.render_finished {
            device.destroy_semaphore(*sem, None);
        }
        device.destroy_semaphore(self.image_available, None);
        device.destroy_command_pool(self.command_pool, None);
        for fb in &self.framebuffers {
            device.destroy_framebuffer(*fb, None);
        }
        device.destroy_pipeline(self.inner, None);
        device.destroy_pipeline_layout(self.layout, None);
        device.destroy_render_pass(self.render_pass, None);
    }
}
