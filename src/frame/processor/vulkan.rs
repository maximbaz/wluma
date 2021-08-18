// Lasciate ogni speranza, voi ch'entrate.
// Desperate attempt to get it working at least. To be cleaned up at some point.

use crate::frame::object::Object;

use ash::{vk, Device, Entry, Instance};
use itertools::Itertools;
use std::default::Default;
use std::error::Error;
use std::ffi::CString;
use std::ops::Drop;

pub struct Processor {
    _entry: Entry,
    instance: Instance,
    device: Device,
    buffer: vk::Buffer,
    buffer_memory: vk::DeviceMemory,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    queue: vk::Queue,
    fence: vk::Fence,
}

impl Processor {
    pub fn new() -> Result<Self, Box<dyn Error>> {
        unsafe {
            let app_name = CString::new("wluma")?;

            let appinfo = vk::ApplicationInfo::builder()
                .application_name(&app_name)
                .application_version(vk::make_api_version(0, 2, 0, 0))
                .engine_name(&app_name)
                .engine_version(vk::make_api_version(0, 2, 0, 0))
                .api_version(vk::make_api_version(0, 1, 0, 0)); // TODO 1.2

            // let extensions = vec![
            // vk::KhrExternalMemoryFn::name().as_ptr(),
            // ash::extensions::khr::ExternalMemoryFd::name().as_ptr(),
            // ash::extensions::khr::GetPhysicalDeviceProperties2::name().as_ptr(),
            // ash::extensions::khr::ExternalFenceFd::name().as_ptr(),
            // ];
            // println!("{:?}", vk::KhrExternalMemoryFn::name());

            let create_info = vk::InstanceCreateInfo::builder().application_info(&appinfo);
            // .enabled_extension_names(&extensions);

            let entry = Entry::new().unwrap();
            let instance = entry.create_instance(&create_info, None)?;

            let physical_device = instance.enumerate_physical_devices()?;
            let physical_device = *physical_device.iter().next().ok_or("no suitable device")?;

            let queue_family_index = 0;
            let queue_info = &[vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(queue_family_index)
                .queue_priorities(&[1.0])
                .build()];

            // let extensions = &[
            //ash::extensions::khr::ExternalMemoryFd::name().as_ptr()
            // ];
            // let features = vk::PhysicalDeviceFeatures::builder();

            let device_create_info = vk::DeviceCreateInfo::builder().queue_create_infos(queue_info);
            // .enabled_extension_names(extensions)
            // .enabled_features(&features);

            let device = instance.create_device(physical_device, &device_create_info, None)?;

            let queue = device.get_device_queue(queue_family_index, 0);

            let pool_create_info = vk::CommandPoolCreateInfo::builder()
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
                .queue_family_index(queue_family_index);

            let command_pool = device.create_command_pool(&pool_create_info, None)?;

            let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::builder()
                .command_buffer_count(1)
                .command_pool(command_pool)
                .level(vk::CommandBufferLevel::PRIMARY);

            let command_buffers = device.allocate_command_buffers(&command_buffer_allocate_info)?;

            let buffer_info = vk::BufferCreateInfo::builder()
                .size(4 * 500)
                .usage(vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);

            let buffer = device.create_buffer(&buffer_info, None)?;

            let buffer_memory_req = device.get_buffer_memory_requirements(buffer);

            let allocate_info = vk::MemoryAllocateInfo {
                allocation_size: buffer_memory_req.size,
                memory_type_index: 0,
                ..Default::default()
            };

            let buffer_memory = device.allocate_memory(&allocate_info, None)?;

            device.bind_buffer_memory(buffer, buffer_memory, 0)?;

            let fence_create_info = vk::FenceCreateInfo::builder();

            let fence = device.create_fence(&fence_create_info, None)?;

            // // let features = Features::none();
            // let features = physical.supported_features();
            // let device_extensions = DeviceExtensions {
            //     ext_external_memory_dma_buf: true,
            //     khr_external_memory_fd: true,
            //     khr_external_memory: true,
            //     ..DeviceExtensions::none()
            // };

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
            })
        }
    }
}

impl Drop for Processor {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();
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

impl super::Processor for Processor {
    fn luma_percent(&self, frame: &Object) -> Result<u8, Box<dyn Error>> {
        unsafe {
            assert_eq!(1, frame.num_objects);

            let mip_levels = f64::max(frame.width.into(), frame.height.into())
                .log2()
                .floor() as u32;

            let image_create_info = vk::ImageCreateInfo::builder()
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::B8G8R8A8_UNORM)
                .extent(vk::Extent3D {
                    width: frame.width / 2,
                    height: frame.height / 2,
                    depth: 1,
                })
                .mip_levels(mip_levels)
                .array_layers(1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .samples(vk::SampleCountFlags::TYPE_1)
                .usage(
                    vk::ImageUsageFlags::TRANSFER_DST
                        | vk::ImageUsageFlags::TRANSFER_SRC
                        | vk::ImageUsageFlags::SAMPLED,
                ) // TODO need sampled?
                .sharing_mode(vk::SharingMode::EXCLUSIVE);

            let image = self.device.create_image(&image_create_info, None)?;
            let image_memory_req = self.device.get_image_memory_requirements(image);

            let image_allocate_info = vk::MemoryAllocateInfo::builder()
                .allocation_size(image_memory_req.size)
                .memory_type_index(0);

            let image_memory = self.device.allocate_memory(&image_allocate_info, None)?;

            self.device.bind_image_memory(image, image_memory, 0)?;

            //////

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
                .flags(vk::ImageCreateFlags::ALIAS)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::SAMPLED) // TODO need sampled?
                .initial_layout(vk::ImageLayout::UNDEFINED)
                .sharing_mode(vk::SharingMode::EXCLUSIVE);

            let frame_image = self.device.create_image(&frame_image_create_info, None)?;

            let mut frame_import_memory_info = vk::ImportMemoryFdInfoKHR::builder()
                .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
                .fd(frame.fds[0]); // TODO dup

            let frame_image_allocate_info = vk::MemoryAllocateInfo::builder()
                .push_next(&mut frame_import_memory_info)
                .allocation_size(frame.sizes[0].into())
                .memory_type_index(0);

            let frame_image_memory = self
                .device
                .allocate_memory(&frame_image_allocate_info, None)?;

            self.device
                .bind_image_memory(frame_image, frame_image_memory, 0)?;

            //////

            let command_buffer_info = vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

            self.device
                .begin_command_buffer(self.command_buffers[0], &command_buffer_info)?;

            let frame_image_barrier = vk::ImageMemoryBarrier::builder()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .image(frame_image)
                .subresource_range(
                    vk::ImageSubresourceRange::builder()
                        .aspect_mask(vk::ImageAspectFlags::COLOR) // TODO color, base level and layer
                        .layer_count(1)
                        .level_count(1)
                        .build(),
                )
                .dst_access_mask(vk::AccessFlags::TRANSFER_READ);

            self.device.cmd_pipeline_barrier(
                self.command_buffers[0],
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[frame_image_barrier.build()],
            );

            let image_barrier = vk::ImageMemoryBarrier::builder()
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .image(image)
                .subresource_range(
                    vk::ImageSubresourceRange::builder()
                        .aspect_mask(vk::ImageAspectFlags::COLOR) // TODO color, base level and layer
                        .level_count(mip_levels)
                        .layer_count(1)
                        .build(),
                )
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);

            self.device.cmd_pipeline_barrier(
                self.command_buffers[0],
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[image_barrier.build()],
            );

            ////

            let blit_info = vk::ImageBlit::builder()
                .src_offsets([
                    vk::Offset3D { x: 0, y: 0, z: 0 },
                    vk::Offset3D {
                        x: frame.width as i32,
                        y: frame.height as i32,
                        z: 1,
                    },
                ])
                .src_subresource(
                    vk::ImageSubresourceLayers::builder()
                        .aspect_mask(vk::ImageAspectFlags::COLOR) // TODO color, base level and layer
                        .mip_level(0)
                        .base_array_layer(0)
                        .layer_count(1)
                        .build(),
                )
                .dst_offsets([
                    vk::Offset3D { x: 0, y: 0, z: 0 },
                    vk::Offset3D {
                        x: frame.width as i32 / 2,
                        y: frame.height as i32 / 2,
                        z: 1,
                    },
                ])
                .dst_subresource(
                    vk::ImageSubresourceLayers::builder()
                        .aspect_mask(vk::ImageAspectFlags::COLOR) // TODO color, base level and layer
                        .mip_level(0)
                        .base_array_layer(0)
                        .layer_count(1)
                        .build(),
                );

            self.device.cmd_blit_image(
                self.command_buffers[0],
                frame_image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[blit_info.build()],
                vk::Filter::LINEAR,
            );

            /////

            let target_mip_level = mip_levels - 4;
            let mut mip_width = frame.width as i32 / 2;
            let mut mip_height = frame.height as i32 / 2;

            for i in 1..=target_mip_level {
                let image_barrier = vk::ImageMemoryBarrier::builder()
                    .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .image(image)
                    .subresource_range(
                        vk::ImageSubresourceRange::builder()
                            .aspect_mask(vk::ImageAspectFlags::COLOR) // TODO color, base level and layer
                            .base_mip_level(i - 1)
                            .level_count(1)
                            .layer_count(1)
                            .build(),
                    )
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_READ);

                self.device.cmd_pipeline_barrier(
                    self.command_buffers[0],
                    vk::PipelineStageFlags::TRANSFER, // TODO
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[image_barrier.build()],
                );

                let next_mip_width = if mip_width > 1 { mip_width / 2 } else { 1 };
                let next_mip_height = if mip_height > 1 { mip_height / 2 } else { 1 };

                let blit_info = vk::ImageBlit::builder()
                    .src_offsets([
                        vk::Offset3D { x: 0, y: 0, z: 0 },
                        vk::Offset3D {
                            x: mip_width,
                            y: mip_height,
                            z: 1,
                        },
                    ])
                    .src_subresource(
                        vk::ImageSubresourceLayers::builder()
                            .aspect_mask(vk::ImageAspectFlags::COLOR) // TODO color, base level and layer
                            .mip_level(i - 1)
                            .base_array_layer(0)
                            .layer_count(1)
                            .build(),
                    )
                    .dst_offsets([
                        vk::Offset3D { x: 0, y: 0, z: 0 },
                        vk::Offset3D {
                            x: next_mip_width,
                            y: next_mip_height,
                            z: 1,
                        },
                    ])
                    .dst_subresource(
                        vk::ImageSubresourceLayers::builder()
                            .aspect_mask(vk::ImageAspectFlags::COLOR) // TODO color, base level and layer
                            .mip_level(i)
                            .base_array_layer(0)
                            .layer_count(1)
                            .build(),
                    );

                self.device.cmd_blit_image(
                    self.command_buffers[0],
                    image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[blit_info.build()],
                    vk::Filter::LINEAR,
                );

                mip_width = next_mip_width;
                mip_height = next_mip_height;
            }

            //////

            let image_barrier = vk::ImageMemoryBarrier::builder()
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                .image(image)
                .subresource_range(
                    vk::ImageSubresourceRange::builder()
                        .aspect_mask(vk::ImageAspectFlags::COLOR) // TODO color, base level and layer
                        .base_mip_level(target_mip_level)
                        .level_count(1)
                        .layer_count(1)
                        .build(),
                )
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::TRANSFER_READ);

            self.device.cmd_pipeline_barrier(
                self.command_buffers[0],
                vk::PipelineStageFlags::TRANSFER, // TODO
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[image_barrier.build()],
            );

            /////

            let buffer_image_copy = vk::BufferImageCopy::builder()
                .buffer_offset(0)
                .buffer_row_length(0)
                .buffer_image_height(0)
                .image_subresource(
                    vk::ImageSubresourceLayers::builder()
                        .aspect_mask(vk::ImageAspectFlags::COLOR) // TODO color, base level and layer
                        .mip_level(target_mip_level)
                        .base_array_layer(0)
                        .layer_count(1)
                        .build(),
                )
                .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                .image_extent(vk::Extent3D {
                    width: mip_width as u32,
                    height: mip_height as u32,
                    depth: 1,
                });

            self.device.cmd_copy_image_to_buffer(
                self.command_buffers[0],
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.buffer,
                &[buffer_image_copy.build()],
            );

            //////

            self.device.end_command_buffer(self.command_buffers[0])?;

            let submit_info = vk::SubmitInfo::builder().command_buffers(&self.command_buffers);

            self.device
                .queue_submit(self.queue, &[submit_info.build()], self.fence)?;

            self.device
                .wait_for_fences(&[self.fence], true, 100000000)?;

            let mem_ptr = self.device.map_memory(
                self.buffer_memory,
                0,
                vk::WHOLE_SIZE,
                vk::MemoryMapFlags::empty(),
            )?;

            let total = (mip_width * mip_height) as f64;
            let s = std::slice::from_raw_parts(mem_ptr as *mut u8, total as usize * 4);

            let (r, g, b) = s
                .iter()
                .chunks(4)
                .into_iter()
                .map(|mut chunk| {
                    let r = chunk.next().unwrap();
                    let g = chunk.next().unwrap();
                    let b = chunk.next().unwrap();
                    (*r as f64, *g as f64, *b as f64)
                })
                .fold1(|(rs, gs, bs), (r, g, b)| (rs + r, gs + g, bs + b))
                .unwrap();
            let (r, g, b) = (r / total, g / total, b / total);
            let result = f64::sqrt(0.241 * r * r + 0.691 * g * g + 0.068 * b * b) / 255.0 * 100.0;

            self.device.unmap_memory(self.buffer_memory);
            self.device.reset_fences(&[self.fence])?;
            self.device.destroy_image(image, None);
            self.device.free_memory(image_memory, None);
            self.device.destroy_image(frame_image, None);
            self.device.free_memory(frame_image_memory, None);

            Ok(result.round() as u8)
        }
    }
}
