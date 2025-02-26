use crate::frame::compute_perceived_lightness_percent;
use crate::frame::object::Object;
use ash::khr::external_memory_fd::Device as KHRDevice;
use ash::{vk, Device, Entry, Instance};
use std::default::Default;
use std::error::Error;
use std::ffi::CString;
use std::ops::Drop;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

const VULKAN_VERSION: u32 = vk::make_api_version(0, 1, 2, 0);

const FINAL_MIP_LEVEL: u32 = 4; // Don't generate mipmaps beyond this level - GPU is doing too poor of a job averaging the colors
const FENCES_TIMEOUT_NS: u64 = 1_000_000_000;

pub struct Vulkan {
    _entry: Entry, // must keep reference to prevent early memory release
    instance: Instance,
    device: Device,
    physical_device: vk::PhysicalDevice,
    khr_device: KHRDevice,
    buffer: Option<vk::Buffer>,
    buffer_memory: Option<vk::DeviceMemory>,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    queue: vk::Queue,
    fence: vk::Fence,
    image: Option<vk::Image>,
    image_memory: Option<vk::DeviceMemory>,
    image_resolution: Option<(u32, u32, u32)>,
    exportable_frame_image: Option<vk::Image>,
    exportable_frame_image_memory: Option<vk::DeviceMemory>,
    exportable_frame_image_fd: Option<OwnedFd>,
}

impl Vulkan {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let app_name = CString::new("wluma")?;
        let app_version: u32 = vk::make_api_version(
            0,
            env!("WLUMA_VERSION_MAJOR").parse()?,
            env!("WLUMA_VERSION_MINOR").parse()?,
            env!("WLUMA_VERSION_PATCH").parse()?,
        );

        let app_info = vk::ApplicationInfo::default()
            .application_name(&app_name)
            .application_version(app_version)
            .engine_name(&app_name)
            .engine_version(app_version)
            .api_version(VULKAN_VERSION);

        let instance_extensions = &[
            vk::KHR_EXTERNAL_MEMORY_CAPABILITIES_NAME.as_ptr(),
            vk::KHR_GET_PHYSICAL_DEVICE_PROPERTIES2_NAME.as_ptr(),
        ];

        let entry = Entry::linked();

        let create_info = vk::InstanceCreateInfo::default()
            .application_info(&app_info)
            .enabled_extension_names(instance_extensions);

        let instance = unsafe {
            entry
                .create_instance(&create_info, None)
                .map_err(anyhow::Error::msg)?
        };

        let physical_devices = unsafe {
            instance
                .enumerate_physical_devices()
                .map_err(anyhow::Error::msg)?
        };
        let physical_device = *physical_devices
            .first()
            .ok_or("Unable to find a physical device")?;

        let queue_family_index = 0;
        let queue_info = &[vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family_index)
            .queue_priorities(&[1.0])];

        let device_extensions = &[
            vk::KHR_EXTERNAL_MEMORY_FD_NAME.as_ptr(),
            vk::EXT_EXTERNAL_MEMORY_DMA_BUF_NAME.as_ptr(),
        ];
        let features = vk::PhysicalDeviceFeatures::default();

        let device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(queue_info)
            .enabled_extension_names(device_extensions)
            .enabled_features(&features);

        let device = unsafe {
            instance
                .create_device(physical_device, &device_create_info, None)
                .map_err(anyhow::Error::msg)?
        };

        let khr_device = KHRDevice::new(&instance, &device);

        let queue = unsafe { device.get_device_queue(queue_family_index, 0) };

        let pool_create_info = vk::CommandPoolCreateInfo::default()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_family_index);

        let command_pool = unsafe {
            device
                .create_command_pool(&pool_create_info, None)
                .map_err(anyhow::Error::msg)?
        };

        let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::default()
            .command_buffer_count(1)
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY);

        let command_buffers = unsafe {
            device
                .allocate_command_buffers(&command_buffer_allocate_info)
                .map_err(anyhow::Error::msg)?
        };

        let fence_create_info = vk::FenceCreateInfo::default();
        let fence = unsafe {
            device
                .create_fence(&fence_create_info, None)
                .map_err(anyhow::Error::msg)?
        };

        Ok(Self {
            _entry: entry,
            instance,
            physical_device,
            device,
            khr_device,
            command_pool,
            command_buffers,
            queue,
            fence,
            image: None,
            image_memory: None,
            image_resolution: None,
            buffer: None,
            buffer_memory: None,
            exportable_frame_image: None,
            exportable_frame_image_memory: None,
            exportable_frame_image_fd: None,
        })
    }

    pub fn luma_percent_from_external_fd(&mut self, frame: &Object) -> Result<u8, Box<dyn Error>> {
        let (frame_image, frame_image_memory) = self.init_frame_image(frame)?;

        let result = self.luma_percent(&frame_image)?;

        unsafe {
            self.device.destroy_image(frame_image, None);
            self.device.free_memory(frame_image_memory, None);
        }

        Ok(result)
    }

    pub fn luma_percent_from_internal_fd(&mut self) -> Result<u8, Box<dyn Error>> {
        let frame_image = self.exportable_frame_image.unwrap();

        let result = self.luma_percent(&frame_image)?;

        Ok(result)
    }

    fn luma_percent(&self, frame_image: &vk::Image) -> Result<u8, Box<dyn Error>> {
        let image = self.image.ok_or("Unable to borrow the Vulkan image")?;
        let buffer_memory = self.buffer_memory.ok_or("Unable to borrow buffer memory")?;

        self.begin_commands()?;

        self.add_barrier(
            frame_image,
            0,
            1,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::AccessFlags::default(),
            vk::AccessFlags::TRANSFER_READ,
            vk::PipelineStageFlags::TOP_OF_PIPE,
        );

        let (target_mip_level, mip_width, mip_height) = self.generate_mipmaps(frame_image, &image);

        self.copy_mipmap(&image, target_mip_level, mip_width, mip_height)?;

        self.submit_commands()?;

        let pixels = mip_width as usize * mip_height as usize;
        let rgbas = unsafe {
            let buffer_pointer = self
                .device
                .map_memory(
                    buffer_memory,
                    0,
                    vk::WHOLE_SIZE,
                    vk::MemoryMapFlags::empty(),
                )
                .map_err(anyhow::Error::msg)?;
            std::slice::from_raw_parts(buffer_pointer as *mut u8, pixels * 4)
        };

        let result = compute_perceived_lightness_percent(rgbas, true, pixels);

        unsafe {
            self.device.unmap_memory(buffer_memory);
        }

        Ok(result)
    }

    fn init_image(&mut self, frame: &Object) -> Result<(), Box<dyn Error>> {
        let mip_levels = f64::max(frame.width.into(), frame.height.into())
            .log2()
            .floor() as u32;

        if let Some((w, h, _)) = self.image_resolution {
            if (w, h) == (frame.width, frame.height) {
                // Image is already initialized, resolution did not change
                return Ok(());
            }
        }

        let image_create_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::R8G8B8A8_UNORM)
            .extent(vk::Extent3D {
                width: frame.width,
                height: frame.height,
                depth: 1,
            })
            .mip_levels(mip_levels)
            .array_layers(1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .samples(vk::SampleCountFlags::TYPE_1)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let image = unsafe {
            self.device
                .create_image(&image_create_info, None)
                .map_err(anyhow::Error::msg)?
        };
        let image_memory_req = unsafe { self.device.get_image_memory_requirements(image) };

        let image_allocate_info = vk::MemoryAllocateInfo::default()
            .allocation_size(image_memory_req.size)
            .memory_type_index(0);

        let image_memory = unsafe {
            self.device
                .allocate_memory(&image_allocate_info, None)
                .map_err(anyhow::Error::msg)?
        };

        unsafe {
            self.device
                .bind_image_memory(image, image_memory, 0)
                .map_err(anyhow::Error::msg)?
        };

        if let Some(old_image) = self.image.replace(image) {
            unsafe {
                self.device.destroy_image(old_image, None);
            }
        }
        if let Some(old_image_memory) = self.image_memory.replace(image_memory) {
            unsafe {
                self.device.free_memory(old_image_memory, None);
            }
        }

        let buffer_size = 4
            * (frame.width >> (mip_levels - FINAL_MIP_LEVEL))
            * (frame.height >> (mip_levels - FINAL_MIP_LEVEL));

        let buffer_info = vk::BufferCreateInfo::default()
            .size(buffer_size as u64)
            .usage(vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe {
            self.device
                .create_buffer(&buffer_info, None)
                .map_err(anyhow::Error::msg)?
        };

        let buffer_memory_req = unsafe { self.device.get_buffer_memory_requirements(buffer) };

        let device_memory_properties = unsafe {
            self.instance
                .get_physical_device_memory_properties(self.physical_device)
        };

        let memory_type_index = find_memory_type_index(
            &buffer_memory_req,
            &device_memory_properties,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )
        .ok_or("Unable to find suitable memory type for the buffer")?;

        let allocate_info = vk::MemoryAllocateInfo {
            allocation_size: buffer_memory_req.size,
            memory_type_index,
            ..Default::default()
        };

        let buffer_memory = unsafe {
            self.device
                .allocate_memory(&allocate_info, None)
                .map_err(anyhow::Error::msg)?
        };

        unsafe {
            self.device
                .bind_buffer_memory(buffer, buffer_memory, 0)
                .map_err(anyhow::Error::msg)?
        };

        if let Some(buffer) = self.buffer.replace(buffer) {
            unsafe {
                self.device.destroy_buffer(buffer, None);
            }
        }
        if let Some(buffer_memory) = self.buffer_memory.replace(buffer_memory) {
            unsafe {
                self.device.free_memory(buffer_memory, None);
            }
        }

        self.image_resolution
            .replace((frame.width, frame.height, mip_levels));

        Ok(())
    }

    fn init_frame_image(
        &mut self,
        frame: &Object,
    ) -> Result<(vk::Image, vk::DeviceMemory), Box<dyn Error>> {
        assert_eq!(
            1, frame.num_objects,
            "Frames with multiple objects are not supported yet, use WLR_DRM_NO_MODIFIERS=1 as described in README and follow issue #8"
        );

        let vk_format = map_drm_format(frame.format);

        assert!(
            vk_format.is_some(),
            "Frame with formats other than DRM_FORMAT_XRGB8888 or DRM_FORMAT_XRGB2101010 are not supported yet (yours is {}). If you see this issue, please open a GitHub issue (unless there's one already open) and share your format value", frame.format
        );

        // External memory info
        let mut frame_image_memory_info = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);

        // Image create info
        let frame_image_create_info = vk::ImageCreateInfo::default()
            .push_next(&mut frame_image_memory_info)
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::B8G8R8A8_UNORM)
            .extent(vk::Extent3D {
                width: frame.width,
                height: frame.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .tiling(vk::ImageTiling::LINEAR)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .samples(vk::SampleCountFlags::TYPE_1)
            .usage(vk::ImageUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let frame_image = unsafe {
            self.device
                .create_image(&frame_image_create_info, None)
                .map_err(anyhow::Error::msg)?
        };

        // Memory requirements info
        let frame_image_memory_req_info =
            vk::ImageMemoryRequirementsInfo2::default().image(frame_image);

        // Prepare the structures to get memory requirements into, then get the memory requirements
        let mut frame_image_mem_dedicated_req = vk::MemoryDedicatedRequirements::default();

        let mut frame_image_mem_req =
            vk::MemoryRequirements2::default().push_next(&mut frame_image_mem_dedicated_req);

        unsafe {
            self.device.get_image_memory_requirements2(
                &frame_image_memory_req_info,
                &mut frame_image_mem_req,
            );
        }

        // Bit i in memory_type_bits is set if the ith memory type in the
        // VkPhysicalDeviceMemoryProperties structure is supported for the image memory.
        // We just use the first type supported (from least significant bit's side)

        // Find suitable memory type index
        let memory_type_index = frame_image_mem_req
            .memory_requirements
            .memory_type_bits
            .trailing_zeros();

        // Import memory app_info
        // Construct the memory alloctation info according to the requirements
        // If the image needs dedicated memory, add MemoryDedicatedAllocateInfo to the info chain
        let mut frame_import_memory_info = vk::ImportMemoryFdInfoKHR::default()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
            .fd(frame.fds[0]);

        // dedicated allocation info
        let mut frame_image_memory_dedicated_info =
            vk::MemoryDedicatedAllocateInfo::default().image(frame_image);

        // Memory allocate info
        let mut frame_image_allocate_info = vk::MemoryAllocateInfo::default()
            .push_next(&mut frame_import_memory_info)
            .allocation_size(frame_image_mem_req.memory_requirements.size)
            .memory_type_index(memory_type_index);

        if frame_image_mem_dedicated_req.prefers_dedicated_allocation == vk::TRUE {
            frame_image_allocate_info =
                frame_image_allocate_info.push_next(&mut frame_image_memory_dedicated_info);
        }

        // Allocate memory and bind it to the image
        let frame_image_memory = unsafe {
            self.device
                .allocate_memory(&frame_image_allocate_info, None)
                .map_err(anyhow::Error::msg)?
        };

        unsafe {
            self.device
                .bind_image_memory(frame_image, frame_image_memory, 0)
                .map_err(anyhow::Error::msg)?;
        };

        // Also ensure the internal image is initialized with the same dimensions
        self.init_image(frame)?;

        Ok((frame_image, frame_image_memory))
    }

    pub fn init_exportable_frame_image(
        &mut self,
        frame: &Object,
    ) -> Result<(i32, u64, u64, u64), Box<dyn Error>> {
        assert_eq!(
            1, frame.num_objects,
            "Frames with multiple objects are not supported yet, use WLR_DRM_NO_MODIFIERS=1 as described in README and follow issue #8"
        );

        let vk_format = map_drm_format(frame.format);

        assert!(
            vk_format.is_some(),
            "Frame with formats other than DRM_FORMAT_XRGB8888 or DRM_FORMAT_XRGB2101010 are not supported yet (yours is {}). If you see this issue, please open a GitHub issue (unless there's one already open) and share your format value", frame.format
        );

        let mut frame_image_memory_info = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);

        let frame_image_create_info = vk::ImageCreateInfo::default()
            .push_next(&mut frame_image_memory_info)
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk_format.unwrap())
            .extent(vk::Extent3D {
                width: frame.width,
                height: frame.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .tiling(vk::ImageTiling::LINEAR)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .samples(vk::SampleCountFlags::TYPE_1)
            .usage(vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let frame_image = unsafe {
            self.device
                .create_image(&frame_image_create_info, None)
                .map_err(anyhow::Error::msg)?
        };

        // Memory requirements info
        let frame_image_memory_req_info =
            vk::ImageMemoryRequirementsInfo2::default().image(frame_image);

        // Prepare the structures to get memory requirements into, then get the memory requirements
        let mut frame_image_mem_dedicated_req = vk::MemoryDedicatedRequirements::default();

        let mut frame_image_mem_req =
            vk::MemoryRequirements2::default().push_next(&mut frame_image_mem_dedicated_req);

        unsafe {
            self.device.get_image_memory_requirements2(
                &frame_image_memory_req_info,
                &mut frame_image_mem_req,
            );
        }

        // Bit i in memory_type_bits is set if the ith memory type in the
        // VkPhysicalDeviceMemoryProperties structure is supported for the image memory.
        // We just use the first type supported (from least significant bit's side)

        // Find suitable memory type index
        let memory_type_index = frame_image_mem_req
            .memory_requirements
            .memory_type_bits
            .trailing_zeros();

        // Specify that the memory can be exported
        let mut frame_import_memory_info = vk::ExportMemoryAllocateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);

        // dedicated allocation info
        let mut frame_image_memory_dedicated_info =
            vk::MemoryDedicatedAllocateInfo::default().image(frame_image);

        // Allocate memory
        let mut frame_image_allocate_info = vk::MemoryAllocateInfo::default()
            .push_next(&mut frame_import_memory_info)
            .allocation_size(frame_image_mem_req.memory_requirements.size)
            .memory_type_index(memory_type_index);

        if frame_image_mem_dedicated_req.prefers_dedicated_allocation == vk::TRUE {
            frame_image_allocate_info =
                frame_image_allocate_info.push_next(&mut frame_image_memory_dedicated_info);
        }

        // Allocate memory and bind it to the image
        let frame_image_memory = unsafe {
            self.device
                .allocate_memory(&frame_image_allocate_info, None)
                .map_err(anyhow::Error::msg)?
        };

        // Bind memory to the image
        unsafe {
            self.device
                .bind_image_memory(frame_image, frame_image_memory, 0)
                .map_err(anyhow::Error::msg)?;
        }

        // Get the file descriptor
        let memory_fd_info = vk::MemoryGetFdInfoKHR::default()
            .memory(frame_image_memory)
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);

        let fd = unsafe {
            OwnedFd::from_raw_fd(
                self.khr_device
                    .get_memory_fd(&memory_fd_info)
                    .map_err(anyhow::Error::msg)?,
            )
        };

        let subresource = vk::ImageSubresource::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .mip_level(0)
            .array_layer(0);

        let layout = unsafe {
            self.device
                .get_image_subresource_layout(frame_image, subresource)
        };

        let offset = layout.offset;
        let stride = layout.row_pitch;
        let modifier: u64 = 0; // DRM_FORMAT_MOD_LINEAR

        let raw_fd = fd.as_raw_fd();

        if let Some(old_image) = self.exportable_frame_image.replace(frame_image) {
            unsafe {
                self.device.destroy_image(old_image, None);
            }
        };

        if let Some(old_image_memory) = self
            .exportable_frame_image_memory
            .replace(frame_image_memory)
        {
            unsafe {
                self.device.free_memory(old_image_memory, None);
            }
        }

        self.exportable_frame_image_fd = Some(fd);

        // Also ensure the internal image is initialized with the same dimensions
        self.init_image(frame)?;

        Ok((raw_fd, offset, stride, modifier))
    }

    #[allow(clippy::too_many_arguments)]
    fn add_barrier(
        &self,
        image: &vk::Image,
        base_mip_level: u32,
        mip_levels: u32,
        old_layout: vk::ImageLayout,
        new_layout: vk::ImageLayout,
        src_access_mask: vk::AccessFlags,
        dst_access_mask: vk::AccessFlags,
        src_stage_mask: vk::PipelineStageFlags,
    ) {
        let image_barrier = vk::ImageMemoryBarrier::default()
            .old_layout(old_layout)
            .new_layout(new_layout)
            .image(*image)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(base_mip_level)
                    .level_count(mip_levels)
                    .layer_count(1),
            )
            .src_access_mask(src_access_mask)
            .dst_access_mask(dst_access_mask);

        unsafe {
            self.device.cmd_pipeline_barrier(
                self.command_buffers[0],
                src_stage_mask,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[image_barrier],
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn blit(
        &self,
        src_image: &vk::Image,
        src_width: u32,
        src_height: u32,
        src_mip_level: u32,
        dst_image: &vk::Image,
        dst_width: u32,
        dst_height: u32,
        dst_mip_level: u32,
    ) {
        let blit_info = vk::ImageBlit::default()
            .src_offsets([
                vk::Offset3D { x: 0, y: 0, z: 0 },
                vk::Offset3D {
                    x: src_width as i32,
                    y: src_height as i32,
                    z: 1,
                },
            ])
            .src_subresource(
                vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .mip_level(src_mip_level)
                    .layer_count(1),
            )
            .dst_offsets([
                vk::Offset3D { x: 0, y: 0, z: 0 },
                vk::Offset3D {
                    x: dst_width as i32,
                    y: dst_height as i32,
                    z: 1,
                },
            ])
            .dst_subresource(
                vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .mip_level(dst_mip_level)
                    .layer_count(1),
            );

        unsafe {
            self.device.cmd_blit_image(
                self.command_buffers[0],
                *src_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                *dst_image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[blit_info],
                vk::Filter::LINEAR,
            );
        }
    }

    fn generate_mipmaps(&self, frame_image: &vk::Image, image: &vk::Image) -> (u32, u32, u32) {
        let (mut mip_width, mut mip_height, mip_levels) = self.image_resolution.unwrap();

        self.add_barrier(
            image,
            0,
            mip_levels,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::AccessFlags::default(),
            vk::AccessFlags::TRANSFER_WRITE,
            vk::PipelineStageFlags::TOP_OF_PIPE,
        );

        self.blit(
            frame_image,
            mip_width,
            mip_height,
            0,
            image,
            mip_width,
            mip_height,
            0,
        );

        let target_mip_level = mip_levels - FINAL_MIP_LEVEL;
        for i in 1..=target_mip_level {
            self.add_barrier(
                image,
                i - 1,
                1,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::AccessFlags::TRANSFER_WRITE,
                vk::AccessFlags::TRANSFER_READ,
                vk::PipelineStageFlags::TRANSFER,
            );

            let next_mip_width = if mip_width > 1 { mip_width / 2 } else { 1 };
            let next_mip_height = if mip_height > 1 { mip_height / 2 } else { 1 };

            self.blit(
                image,
                mip_width,
                mip_height,
                i - 1,
                image,
                next_mip_width,
                next_mip_height,
                i,
            );

            mip_width = next_mip_width;
            mip_height = next_mip_height;
        }

        (target_mip_level, mip_width, mip_height)
    }

    fn copy_mipmap(
        &self,
        image: &vk::Image,
        mip_level: u32,
        width: u32,
        height: u32,
    ) -> Result<(), Box<dyn Error>> {
        self.add_barrier(
            image,
            mip_level,
            1,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::AccessFlags::TRANSFER_WRITE,
            vk::AccessFlags::TRANSFER_READ,
            vk::PipelineStageFlags::TRANSFER,
        );

        let buffer_image_copy = vk::BufferImageCopy::default()
            .image_subresource(
                vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .mip_level(mip_level)
                    .layer_count(1),
            )
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            });

        let buffer = self.buffer.ok_or("Unable to borrow buffer")?;

        unsafe {
            self.device.cmd_copy_image_to_buffer(
                self.command_buffers[0],
                *image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                buffer,
                &[buffer_image_copy],
            );
        }

        Ok(())
    }

    fn begin_commands(&self) -> Result<(), Box<dyn Error>> {
        let command_buffer_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        unsafe {
            self.device
                .begin_command_buffer(self.command_buffers[0], &command_buffer_info)
                .map_err(anyhow::Error::msg)?;
        }

        Ok(())
    }

    fn submit_commands(&self) -> Result<(), Box<dyn Error>> {
        unsafe {
            // End the command buffer
            self.device
                .end_command_buffer(self.command_buffers[0])
                .map_err(anyhow::Error::msg)?;
        };

        let submit_info = vk::SubmitInfo::default().command_buffers(&self.command_buffers);

        unsafe {
            // Submit the command buffers to the queue
            self.device
                .queue_submit(self.queue, &[submit_info], self.fence)
                .map_err(anyhow::Error::msg)?;

            // Wait for the fences
            self.device
                .wait_for_fences(&[self.fence], true, FENCES_TIMEOUT_NS)
                .map_err(anyhow::Error::msg)?;

            // Reset fences
            self.device
                .reset_fences(&[self.fence])
                .map_err(anyhow::Error::msg)?;
        }

        Ok(())
    }
}

impl Drop for Vulkan {
    fn drop(&mut self) {
        unsafe {
            self.device
                .device_wait_idle()
                .expect("Unable to wait for device to become idle");

            if let Some(image) = self.image {
                self.device.destroy_image(image, None);
            }
            if let Some(image_memory) = self.image_memory {
                self.device.free_memory(image_memory, None);
            }

            self.device.destroy_fence(self.fence, None);
            if let Some(buffer) = self.buffer {
                self.device.destroy_buffer(buffer, None);
            }
            if let Some(buffer_memory) = self.buffer_memory {
                self.device.free_memory(buffer_memory, None);
            }
            self.device
                .free_command_buffers(self.command_pool, &self.command_buffers);
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

fn find_memory_type_index(
    memory_req: &vk::MemoryRequirements,
    memory_prop: &vk::PhysicalDeviceMemoryProperties,
    flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    memory_prop.memory_types[..memory_prop.memory_type_count as _]
        .iter()
        .enumerate()
        .find(|(index, memory_type)| {
            (1 << index) & memory_req.memory_type_bits != 0
                && memory_type.property_flags & flags == flags
        })
        .map(|(index, _)| index as _)
}

fn map_drm_format(format: u32) -> Option<vk::Format> {
    match format {
        875713112 => Some(vk::Format::B8G8R8A8_UNORM),
        808669784 => Some(vk::Format::A2R10G10B10_UNORM_PACK32),
        _ => None,
    }
}
