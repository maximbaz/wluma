use crate::frame::compute_perceived_lightness_percent;
use crate::frame::object::Object;
use ash::{vk, Device, Entry, Instance};
use std::cell::RefCell;
use std::default::Default;
use std::error::Error;
use std::ffi::CString;
use std::ops::Drop;

const WLUMA_VERSION: u32 = vk::make_api_version(0, 2, 0, 1);
const VULKAN_VERSION: u32 = vk::make_api_version(0, 1, 2, 0);

const FINAL_MIP_LEVEL: u32 = 4; // Don't generate mipmaps beyond this level - GPU is doing too poor of a job averaging the colors
const BUFFER_PIXELS: u64 = 500 * 4; // Pre-allocated buffer size, should be enough to fit FINAL_MIP_LEVEL
const FENCES_TIMEOUT_NS: u64 = 1_000_000_000;

pub struct Vulkan {
    _entry: Entry, // must keep reference to prevent early memory release
    instance: Instance,
    device: Device,
    buffer: vk::Buffer,
    buffer_memory: vk::DeviceMemory,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    queue: vk::Queue,
    fence: vk::Fence,
    image: RefCell<Option<vk::Image>>,
    image_memory: RefCell<Option<vk::DeviceMemory>>,
    image_resolution: RefCell<Option<(u32, u32)>>,
}

impl Vulkan {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        let app_name = CString::new("wluma")?;
        let app_info = vk::ApplicationInfo::builder()
            .application_name(&app_name)
            .application_version(WLUMA_VERSION)
            .engine_name(&app_name)
            .engine_version(WLUMA_VERSION)
            .api_version(VULKAN_VERSION);

        let instance_extensions = &[
            vk::KhrExternalMemoryCapabilitiesFn::name().as_ptr(),
            vk::KhrGetPhysicalDeviceProperties2Fn::name().as_ptr(),
        ];

        let entry = unsafe { Entry::load()? };

        let create_info = vk::InstanceCreateInfo::builder()
            .application_info(&app_info)
            .enabled_extension_names(instance_extensions);

        let instance = unsafe { entry.create_instance(&create_info, None)? };

        let physical_devices = unsafe { instance.enumerate_physical_devices()? };
        let physical_device = *physical_devices
            .get(0)
            .ok_or("Unable to find a physical device")?;

        let queue_family_index = 0;
        let queue_info = &[vk::DeviceQueueCreateInfo::builder()
            .queue_family_index(queue_family_index)
            .queue_priorities(&[1.0])
            .build()];

        let device_extensions = &[
            vk::KhrExternalMemoryFn::name().as_ptr(),
            vk::KhrExternalMemoryFdFn::name().as_ptr(),
            vk::ExtExternalMemoryDmaBufFn::name().as_ptr(),
        ];
        let features = vk::PhysicalDeviceFeatures::builder();

        let device_create_info = vk::DeviceCreateInfo::builder()
            .queue_create_infos(queue_info)
            .enabled_extension_names(device_extensions)
            .enabled_features(&features);

        let device = unsafe { instance.create_device(physical_device, &device_create_info, None)? };

        let queue = unsafe { device.get_device_queue(queue_family_index, 0) };

        let pool_create_info = vk::CommandPoolCreateInfo::builder()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_family_index);

        let command_pool = unsafe { device.create_command_pool(&pool_create_info, None)? };

        let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_buffer_count(1)
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY);
        let command_buffers =
            unsafe { device.allocate_command_buffers(&command_buffer_allocate_info)? };

        let buffer_info = vk::BufferCreateInfo::builder()
            .size(BUFFER_PIXELS)
            .usage(vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe { device.create_buffer(&buffer_info, None)? };

        let buffer_memory_req = unsafe { device.get_buffer_memory_requirements(buffer) };

        let device_memory_properties =
            unsafe { instance.get_physical_device_memory_properties(physical_device) };

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

        let buffer_memory = unsafe { device.allocate_memory(&allocate_info, None)? };
        unsafe {
            device.bind_buffer_memory(buffer, buffer_memory, 0)?;
        }

        let fence_create_info = vk::FenceCreateInfo::builder();
        let fence = unsafe { device.create_fence(&fence_create_info, None)? };

        Ok(Self {
            _entry: entry,
            instance,
            device,
            buffer,
            buffer_memory,
            command_pool,
            command_buffers,
            queue,
            fence,
            image: RefCell::new(None),
            image_memory: RefCell::new(None),
            image_resolution: RefCell::new(None),
        })
    }

    pub fn luma_percent(&self, frame: &Object) -> Result<u8, Box<dyn Error>> {
        assert_eq!(
            1, frame.num_objects,
            "Frames with multiple objects are not supported yet"
        );

        if self.image.borrow().is_none() {
            self.init_image(frame)?;
        }
        assert_eq!(
            (frame.width, frame.height),
            self.image_resolution.borrow().unwrap(),
            "Handling screen resolution change is not supported yet"
        );

        let image = self
            .image
            .borrow()
            .ok_or("Unable to borrow the Vulkan image")?;

        let (frame_image, frame_image_memory) = self.init_frame_image(frame)?;

        self.begin_commands()?;

        let (target_mip_level, mip_width, mip_height) =
            self.generate_mipmaps(frame, &frame_image, &image);

        self.copy_mipmap(&image, target_mip_level, mip_width, mip_height);

        self.submit_commands()?;

        let pixels = mip_width as usize * mip_height as usize;
        let rgbas = unsafe {
            let buffer_pointer = self.device.map_memory(
                self.buffer_memory,
                0,
                vk::WHOLE_SIZE,
                vk::MemoryMapFlags::empty(),
            )?;
            std::slice::from_raw_parts(buffer_pointer as *mut u8, pixels * 4)
        };

        let result = compute_perceived_lightness_percent(rgbas, true, pixels);

        unsafe {
            self.device.unmap_memory(self.buffer_memory);
            self.device.reset_fences(&[self.fence])?;
            self.device.destroy_image(frame_image, None);
            self.device.free_memory(frame_image_memory, None);
        }

        Ok(result)
    }

    fn init_image(&self, frame: &Object) -> Result<(), Box<dyn Error>> {
        let (width, height, mip_levels) = image_dimensions(frame);

        let image_create_info = vk::ImageCreateInfo::builder()
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::B8G8R8A8_UNORM)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(mip_levels)
            .array_layers(1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .samples(vk::SampleCountFlags::TYPE_1)
            .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let image = unsafe { self.device.create_image(&image_create_info, None)? };
        let image_memory_req = unsafe { self.device.get_image_memory_requirements(image) };

        let image_allocate_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(image_memory_req.size)
            .memory_type_index(0);

        let image_memory = unsafe { self.device.allocate_memory(&image_allocate_info, None)? };

        unsafe {
            self.device.bind_image_memory(image, image_memory, 0)?;
        }

        self.image.borrow_mut().replace(image);
        self.image_memory.borrow_mut().replace(image_memory);
        self.image_resolution
            .borrow_mut()
            .replace((frame.width, frame.height));
        Ok(())
    }

    fn init_frame_image(
        &self,
        frame: &Object,
    ) -> Result<(vk::Image, vk::DeviceMemory), Box<dyn Error>> {
        let mut frame_image_memory_info = vk::ExternalMemoryImageCreateInfo::builder()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);

        let frame_image_create_info = vk::ImageCreateInfo::builder()
            .push_next(&mut frame_image_memory_info)
            .image_type(vk::ImageType::TYPE_2D)
            .format(vk::Format::R8G8B8A8_UNORM)
            .extent(vk::Extent3D {
                width: frame.width,
                height: frame.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .samples(vk::SampleCountFlags::TYPE_1)
            .usage(vk::ImageUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let frame_image = unsafe { self.device.create_image(&frame_image_create_info, None)? };

        let mut frame_import_memory_info = vk::ImportMemoryFdInfoKHR::builder()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
            .fd(frame.fds[0]);

        let frame_image_allocate_info = vk::MemoryAllocateInfo::builder()
            .push_next(&mut frame_import_memory_info)
            .allocation_size(frame.sizes[0].into())
            .memory_type_index(0);

        let frame_image_memory = unsafe {
            self.device
                .allocate_memory(&frame_image_allocate_info, None)?
        };

        unsafe {
            self.device
                .bind_image_memory(frame_image, frame_image_memory, 0)?;
        }

        Ok((frame_image, frame_image_memory))
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
        let image_barrier = vk::ImageMemoryBarrier::builder()
            .old_layout(old_layout)
            .new_layout(new_layout)
            .image(*image)
            .subresource_range(
                vk::ImageSubresourceRange::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .base_mip_level(base_mip_level)
                    .level_count(mip_levels)
                    .layer_count(1)
                    .build(),
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
                &[image_barrier.build()],
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
        let blit_info = vk::ImageBlit::builder()
            .src_offsets([
                vk::Offset3D { x: 0, y: 0, z: 0 },
                vk::Offset3D {
                    x: src_width as i32,
                    y: src_height as i32,
                    z: 1,
                },
            ])
            .src_subresource(
                vk::ImageSubresourceLayers::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .mip_level(src_mip_level)
                    .layer_count(1)
                    .build(),
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
                vk::ImageSubresourceLayers::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .mip_level(dst_mip_level)
                    .layer_count(1)
                    .build(),
            );

        unsafe {
            self.device.cmd_blit_image(
                self.command_buffers[0],
                *src_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                *dst_image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[blit_info.build()],
                vk::Filter::LINEAR,
            );
        }
    }

    fn generate_mipmaps(
        &self,
        frame: &Object,
        frame_image: &vk::Image,
        image: &vk::Image,
    ) -> (u32, u32, u32) {
        let (mut mip_width, mut mip_height, mip_levels) = image_dimensions(frame);

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
            frame.width,
            frame.height,
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

    fn copy_mipmap(&self, image: &vk::Image, mip_level: u32, width: u32, height: u32) {
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

        let buffer_image_copy = vk::BufferImageCopy::builder()
            .image_subresource(
                vk::ImageSubresourceLayers::builder()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .mip_level(mip_level)
                    .layer_count(1)
                    .build(),
            )
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            });

        unsafe {
            self.device.cmd_copy_image_to_buffer(
                self.command_buffers[0],
                *image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.buffer,
                &[buffer_image_copy.build()],
            );
        }
    }

    fn begin_commands(&self) -> Result<(), Box<dyn Error>> {
        let command_buffer_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        unsafe {
            self.device
                .begin_command_buffer(self.command_buffers[0], &command_buffer_info)?;
        }

        Ok(())
    }

    fn submit_commands(&self) -> Result<(), Box<dyn Error>> {
        unsafe {
            self.device.end_command_buffer(self.command_buffers[0])?;
        }

        let submit_info = vk::SubmitInfo::builder().command_buffers(&self.command_buffers);

        unsafe {
            self.device
                .queue_submit(self.queue, &[submit_info.build()], self.fence)?;
            self.device
                .wait_for_fences(&[self.fence], true, FENCES_TIMEOUT_NS)?;
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

            if let Some(image) = *self.image.borrow() {
                self.device.destroy_image(image, None);
            }
            if let Some(image_memory) = *self.image_memory.borrow() {
                self.device.free_memory(image_memory, None);
            }

            self.device.destroy_fence(self.fence, None);
            self.device.destroy_buffer(self.buffer, None);
            self.device.free_memory(self.buffer_memory, None);
            self.device
                .free_command_buffers(self.command_pool, &self.command_buffers);
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

fn image_dimensions(frame: &Object) -> (u32, u32, u32) {
    let width = frame.width / 2;
    let height = frame.height / 2;
    let mip_levels = f64::max(width.into(), height.into()).log2().floor() as u32 + 1;
    (width, height, mip_levels)
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
