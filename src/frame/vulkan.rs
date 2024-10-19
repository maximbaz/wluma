use crate::frame::compute_perceived_lightness_percent;
use crate::frame::object::Object;
use ash::{vk, Device, Entry, Instance};
use std::cell::RefCell;
use std::default::Default;
use std::error::Error;
use std::ffi::CString;
use std::ops::Drop;

const WLUMA_VERSION: u32 = vk::make_api_version(0, 4, 4, 0);
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
        let app_info = vk::ApplicationInfo::default()
            .application_name(&app_name)
            .application_version(WLUMA_VERSION)
            .engine_name(&app_name)
            .engine_version(WLUMA_VERSION)
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

        let formats = vec![
            vk::Format::R4G4_UNORM_PACK8,
            vk::Format::R4G4B4A4_UNORM_PACK16,
            vk::Format::B4G4R4A4_UNORM_PACK16,
            vk::Format::R5G6B5_UNORM_PACK16,
            vk::Format::B5G6R5_UNORM_PACK16,
            vk::Format::R5G5B5A1_UNORM_PACK16,
            vk::Format::B5G5R5A1_UNORM_PACK16,
            vk::Format::A1R5G5B5_UNORM_PACK16,
            vk::Format::R8_UNORM,
            vk::Format::R8_SNORM,
            vk::Format::R8_USCALED,
            vk::Format::R8_SSCALED,
            vk::Format::R8_UINT,
            vk::Format::R8_SINT,
            vk::Format::R8_SRGB,
            vk::Format::R8G8_UNORM,
            vk::Format::R8G8_SNORM,
            vk::Format::R8G8_USCALED,
            vk::Format::R8G8_SSCALED,
            vk::Format::R8G8_UINT,
            vk::Format::R8G8_SINT,
            vk::Format::R8G8_SRGB,
            vk::Format::R8G8B8_UNORM,
            vk::Format::R8G8B8_SNORM,
            vk::Format::R8G8B8_USCALED,
            vk::Format::R8G8B8_SSCALED,
            vk::Format::R8G8B8_UINT,
            vk::Format::R8G8B8_SINT,
            vk::Format::R8G8B8_SRGB,
            vk::Format::B8G8R8_UNORM,
            vk::Format::B8G8R8_SNORM,
            vk::Format::B8G8R8_USCALED,
            vk::Format::B8G8R8_SSCALED,
            vk::Format::B8G8R8_UINT,
            vk::Format::B8G8R8_SINT,
            vk::Format::B8G8R8_SRGB,
            vk::Format::R8G8B8A8_UNORM,
            vk::Format::R8G8B8A8_SNORM,
            vk::Format::R8G8B8A8_USCALED,
            vk::Format::R8G8B8A8_SSCALED,
            vk::Format::R8G8B8A8_UINT,
            vk::Format::R8G8B8A8_SINT,
            vk::Format::R8G8B8A8_SRGB,
            vk::Format::B8G8R8A8_UNORM,
            vk::Format::B8G8R8A8_SNORM,
            vk::Format::B8G8R8A8_USCALED,
            vk::Format::B8G8R8A8_SSCALED,
            vk::Format::B8G8R8A8_UINT,
            vk::Format::B8G8R8A8_SINT,
            vk::Format::B8G8R8A8_SRGB,
            vk::Format::A8B8G8R8_UNORM_PACK32,
            vk::Format::A8B8G8R8_SNORM_PACK32,
            vk::Format::A8B8G8R8_USCALED_PACK32,
            vk::Format::A8B8G8R8_SSCALED_PACK32,
            vk::Format::A8B8G8R8_UINT_PACK32,
            vk::Format::A8B8G8R8_SINT_PACK32,
            vk::Format::A8B8G8R8_SRGB_PACK32,
            vk::Format::A2R10G10B10_UNORM_PACK32,
            vk::Format::A2R10G10B10_SNORM_PACK32,
            vk::Format::A2R10G10B10_USCALED_PACK32,
            vk::Format::A2R10G10B10_SSCALED_PACK32,
            vk::Format::A2R10G10B10_UINT_PACK32,
            vk::Format::A2R10G10B10_SINT_PACK32,
            vk::Format::A2B10G10R10_UNORM_PACK32,
            vk::Format::A2B10G10R10_SNORM_PACK32,
            vk::Format::A2B10G10R10_USCALED_PACK32,
            vk::Format::A2B10G10R10_SSCALED_PACK32,
            vk::Format::A2B10G10R10_UINT_PACK32,
            vk::Format::A2B10G10R10_SINT_PACK32,
            vk::Format::R16_UNORM,
            vk::Format::R16_SNORM,
            vk::Format::R16_USCALED,
            vk::Format::R16_SSCALED,
            vk::Format::R16_UINT,
            vk::Format::R16_SINT,
            vk::Format::R16_SFLOAT,
            vk::Format::R16G16_UNORM,
            vk::Format::R16G16_SNORM,
            vk::Format::R16G16_USCALED,
            vk::Format::R16G16_SSCALED,
            vk::Format::R16G16_UINT,
            vk::Format::R16G16_SINT,
            vk::Format::R16G16_SFLOAT,
            vk::Format::R16G16B16_UNORM,
            vk::Format::R16G16B16_SNORM,
            vk::Format::R16G16B16_USCALED,
            vk::Format::R16G16B16_SSCALED,
            vk::Format::R16G16B16_UINT,
            vk::Format::R16G16B16_SINT,
            vk::Format::R16G16B16_SFLOAT,
            vk::Format::R16G16B16A16_UNORM,
            vk::Format::R16G16B16A16_SNORM,
            vk::Format::R16G16B16A16_USCALED,
            vk::Format::R16G16B16A16_SSCALED,
            vk::Format::R16G16B16A16_UINT,
            vk::Format::R16G16B16A16_SINT,
            vk::Format::R16G16B16A16_SFLOAT,
            vk::Format::R32_UINT,
            vk::Format::R32_SINT,
            vk::Format::R32_SFLOAT,
            vk::Format::R32G32_UINT,
            vk::Format::R32G32_SINT,
            vk::Format::R32G32_SFLOAT,
            vk::Format::R32G32B32_UINT,
            vk::Format::R32G32B32_SINT,
            vk::Format::R32G32B32_SFLOAT,
            vk::Format::R32G32B32A32_UINT,
            vk::Format::R32G32B32A32_SINT,
            vk::Format::R32G32B32A32_SFLOAT,
            vk::Format::R64_UINT,
            vk::Format::R64_SINT,
            vk::Format::R64_SFLOAT,
            vk::Format::R64G64_UINT,
            vk::Format::R64G64_SINT,
            vk::Format::R64G64_SFLOAT,
            vk::Format::R64G64B64_UINT,
            vk::Format::R64G64B64_SINT,
            vk::Format::R64G64B64_SFLOAT,
            vk::Format::R64G64B64A64_UINT,
            vk::Format::R64G64B64A64_SINT,
            vk::Format::R64G64B64A64_SFLOAT,
            vk::Format::B10G11R11_UFLOAT_PACK32,
            vk::Format::E5B9G9R9_UFLOAT_PACK32,
            vk::Format::D16_UNORM,
            vk::Format::X8_D24_UNORM_PACK32,
            vk::Format::D32_SFLOAT,
            vk::Format::S8_UINT,
            vk::Format::D16_UNORM_S8_UINT,
            vk::Format::D24_UNORM_S8_UINT,
            vk::Format::D32_SFLOAT_S8_UINT,
            vk::Format::BC1_RGB_UNORM_BLOCK,
            vk::Format::BC1_RGB_SRGB_BLOCK,
            vk::Format::BC1_RGBA_UNORM_BLOCK,
            vk::Format::BC1_RGBA_SRGB_BLOCK,
            vk::Format::BC2_UNORM_BLOCK,
            vk::Format::BC2_SRGB_BLOCK,
            vk::Format::BC3_UNORM_BLOCK,
            vk::Format::BC3_SRGB_BLOCK,
            vk::Format::BC4_UNORM_BLOCK,
            vk::Format::BC4_SNORM_BLOCK,
            vk::Format::BC5_UNORM_BLOCK,
            vk::Format::BC5_SNORM_BLOCK,
            vk::Format::BC6H_UFLOAT_BLOCK,
            vk::Format::BC6H_SFLOAT_BLOCK,
            vk::Format::BC7_UNORM_BLOCK,
            vk::Format::BC7_SRGB_BLOCK,
            vk::Format::ETC2_R8G8B8_UNORM_BLOCK,
            vk::Format::ETC2_R8G8B8_SRGB_BLOCK,
            vk::Format::ETC2_R8G8B8A1_UNORM_BLOCK,
            vk::Format::ETC2_R8G8B8A1_SRGB_BLOCK,
            vk::Format::ETC2_R8G8B8A8_UNORM_BLOCK,
            vk::Format::ETC2_R8G8B8A8_SRGB_BLOCK,
            vk::Format::EAC_R11_UNORM_BLOCK,
            vk::Format::EAC_R11_SNORM_BLOCK,
            vk::Format::EAC_R11G11_UNORM_BLOCK,
            vk::Format::EAC_R11G11_SNORM_BLOCK,
            vk::Format::ASTC_4X4_UNORM_BLOCK,
            vk::Format::ASTC_4X4_SRGB_BLOCK,
            vk::Format::ASTC_5X4_UNORM_BLOCK,
            vk::Format::ASTC_5X4_SRGB_BLOCK,
            vk::Format::ASTC_5X5_UNORM_BLOCK,
            vk::Format::ASTC_5X5_SRGB_BLOCK,
            vk::Format::ASTC_6X5_UNORM_BLOCK,
            vk::Format::ASTC_6X5_SRGB_BLOCK,
            vk::Format::ASTC_6X6_UNORM_BLOCK,
            vk::Format::ASTC_6X6_SRGB_BLOCK,
            vk::Format::ASTC_8X5_UNORM_BLOCK,
            vk::Format::ASTC_8X5_SRGB_BLOCK,
            vk::Format::ASTC_8X6_UNORM_BLOCK,
            vk::Format::ASTC_8X6_SRGB_BLOCK,
            vk::Format::ASTC_8X8_UNORM_BLOCK,
            vk::Format::ASTC_8X8_SRGB_BLOCK,
            vk::Format::ASTC_10X5_UNORM_BLOCK,
            vk::Format::ASTC_10X5_SRGB_BLOCK,
            vk::Format::ASTC_10X6_UNORM_BLOCK,
            vk::Format::ASTC_10X6_SRGB_BLOCK,
            vk::Format::ASTC_10X8_UNORM_BLOCK,
            vk::Format::ASTC_10X8_SRGB_BLOCK,
            vk::Format::ASTC_10X10_UNORM_BLOCK,
            vk::Format::ASTC_10X10_SRGB_BLOCK,
            vk::Format::ASTC_12X10_UNORM_BLOCK,
            vk::Format::ASTC_12X10_SRGB_BLOCK,
            vk::Format::ASTC_12X12_UNORM_BLOCK,
            vk::Format::ASTC_12X12_SRGB_BLOCK,
        ];

        for format in formats {
            let mut pdeifi = vk::PhysicalDeviceExternalImageFormatInfo::default()
                .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);

            let pdifi = vk::PhysicalDeviceImageFormatInfo2::default()
                .push_next(&mut pdeifi)
                .ty(vk::ImageType::TYPE_2D)
                .format(format)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::TRANSFER_SRC);
            let mut ifp = vk::ImageFormatProperties2::default();

            let res = unsafe {
                instance.get_physical_device_image_format_properties2(
                    physical_device,
                    &pdifi,
                    &mut ifp,
                )
            };
            println!("====> {} => {:?}", format.as_raw(), res);
        }

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

        let buffer_info = vk::BufferCreateInfo::default()
            .size(BUFFER_PIXELS)
            .usage(vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = unsafe {
            device
                .create_buffer(&buffer_info, None)
                .map_err(anyhow::Error::msg)?
        };

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

        let buffer_memory = unsafe {
            device
                .allocate_memory(&allocate_info, None)
                .map_err(anyhow::Error::msg)?
        };

        unsafe {
            device
                .bind_buffer_memory(buffer, buffer_memory, 0)
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
            "Frames with multiple objects are not supported yet, use WLR_DRM_NO_MODIFIERS=1 as described in README and follow issue #8"
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
            let buffer_pointer = self
                .device
                .map_memory(
                    self.buffer_memory,
                    0,
                    vk::WHOLE_SIZE,
                    vk::MemoryMapFlags::empty(),
                )
                .map_err(anyhow::Error::msg)?;
            std::slice::from_raw_parts(buffer_pointer as *mut u8, pixels * 4)
        };

        let result = compute_perceived_lightness_percent(rgbas, true, pixels);

        unsafe {
            self.device.unmap_memory(self.buffer_memory);
            self.device
                .reset_fences(&[self.fence])
                .map_err(anyhow::Error::msg)?;
            self.device.destroy_image(frame_image, None);
            self.device.free_memory(frame_image_memory, None);
        }

        Ok(result)
    }

    fn init_image(&self, frame: &Object) -> Result<(), Box<dyn Error>> {
        let (width, height, mip_levels) = image_dimensions(frame);

        let image_create_info = vk::ImageCreateInfo::default()
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
        // External memory info
        let mut frame_image_memory_info = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);

        // Image create info
        let frame_image_create_info = vk::ImageCreateInfo::default()
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

        unsafe {
            self.device.cmd_copy_image_to_buffer(
                self.command_buffers[0],
                *image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.buffer,
                &[buffer_image_copy],
            );
        }
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
