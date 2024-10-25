use crate::config::WaylandProtocol;
use crate::frame::object::Object;
use crate::frame::vulkan::Vulkan;
use crate::predictor::Controller;
use std::os::fd::BorrowedFd;
use std::thread;
use std::time::Duration;
use wayland_client::event_created_child;
use wayland_client::protocol::wl_buffer;
use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_client::protocol::wl_output;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::protocol::wl_registry;
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::Connection;
use wayland_client::Dispatch;
use wayland_client::QueueHandle;
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1;
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1::Flags;
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1::ZwpLinuxBufferParamsV1;
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_dmabuf_v1;
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1;
use wayland_protocols_wlr::export_dmabuf::v1::client::zwlr_export_dmabuf_frame_v1;
use wayland_protocols_wlr::export_dmabuf::v1::client::zwlr_export_dmabuf_frame_v1::ZwlrExportDmabufFrameV1;
use wayland_protocols_wlr::export_dmabuf::v1::client::zwlr_export_dmabuf_manager_v1;
use wayland_protocols_wlr::export_dmabuf::v1::client::zwlr_export_dmabuf_manager_v1::ZwlrExportDmabufManagerV1;
use wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_frame_v1;
use wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_frame_v1::ZwlrScreencopyFrameV1;
use wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_manager_v1;
use wayland_protocols_wlr::screencopy::v1::client::zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1;

const DELAY_SUCCESS: Duration = Duration::from_millis(100);
const DELAY_FAILURE: Duration = Duration::from_millis(1000);

pub struct Capturer {
    protocol: WaylandProtocol,
    is_processing_frame: bool,
    vulkan: Option<Vulkan>,
    output: Option<WlOutput>,
    output_global_id: Option<u32>,
    pending_frame: Option<Object>,
    controller: Option<Controller>,
    // wlr-screencopy-unstable-v1
    screencopy_manager: Option<ZwlrScreencopyManagerV1>,
    dmabuf: Option<ZwpLinuxDmabufV1>,
    screencopy_frame: Option<ZwlrScreencopyFrameV1>,
    wl_buffer: Option<WlBuffer>,
    dmabuf_params: Option<ZwpLinuxBufferParamsV1>,
    // wlr-export-dmabuf-unstable-v1
    dmabuf_manager: Option<ZwlrExportDmabufManagerV1>,
}

#[derive(Clone)]
struct GlobalsContext {
    global_id: Option<u32>,
    desired_output: String,
}

impl Capturer {
    pub fn new(protocol: WaylandProtocol) -> Self {
        Self {
            protocol,
            is_processing_frame: false,
            vulkan: None,
            output: None,
            output_global_id: None,
            screencopy_manager: None,
            dmabuf: None,
            screencopy_frame: None,
            wl_buffer: None,
            dmabuf_manager: None,
            pending_frame: None,
            dmabuf_params: None,
            controller: None,
        }
    }
}

impl super::Capturer for Capturer {
    fn run(&mut self, output_name: &str, controller: Controller) {
        let connection =
            Connection::connect_to_env().expect("Unable to connect to Wayland display");
        let display = connection.display();
        let mut event_queue = connection.new_event_queue();
        let qh = event_queue.handle();

        let ctx = GlobalsContext {
            global_id: None,
            desired_output: output_name.to_string(),
        };

        display.get_registry(&qh, ctx);

        // 1. process registry events
        event_queue
            .roundtrip(self)
            .expect("Unable to perform initial roundtrip");

        // 2. registry requested wl_output events, process those
        event_queue
            .roundtrip(self)
            .expect("Unable to perform 2nd initial roundtrip");

        let protocol_to_use = match self.protocol {
            WaylandProtocol::WlrScreencopyUnstableV1 => {
                if self.screencopy_manager.is_none() {
                    panic!("Requested to use wlr-screencopy-unstable-v1 protocol, but it's not available");
                }
                log::debug!("Using wlr-screencopy-unstable-v1 protocol to request frames");
                WaylandProtocol::WlrScreencopyUnstableV1
            }
            WaylandProtocol::WlrExportDmabufUnstableV1 => {
                if self.dmabuf_manager.is_none() {
                    panic!("Requested to use wlr-export-dmabuf-unstable-v1 protocol, but it's not available");
                }
                log::debug!("Using wlr-export-dmabuf-unstable-v1 protocol to request frames");
                WaylandProtocol::WlrExportDmabufUnstableV1
            }
            WaylandProtocol::Any => {
                if self.screencopy_manager.is_some() {
                    log::debug!("Using wlr-screencopy-unstable-v1 protocol to request frames");
                    WaylandProtocol::WlrScreencopyUnstableV1
                } else if self.dmabuf_manager.is_some() {
                    log::debug!("Using wlr-export-dmabuf-unstable-v1 protocol to request frames");
                    WaylandProtocol::WlrExportDmabufUnstableV1
                } else {
                    panic!("No supported Wayland protocols found to capture screen contents");
                }
            }
        };

        self.vulkan = Some(Vulkan::new().expect("Unable to initialize Vulkan"));
        self.controller = Some(controller);

        loop {
            if !self.is_processing_frame {
                if let Some(output) = self.output.as_mut() {
                    self.is_processing_frame = true;

                    match protocol_to_use {
                        WaylandProtocol::WlrScreencopyUnstableV1 => {
                            self.screencopy_manager.as_mut().unwrap().capture_output(
                                0,
                                output,
                                &event_queue.handle(),
                                (),
                            );
                        }
                        WaylandProtocol::WlrExportDmabufUnstableV1 => {
                            self.dmabuf_manager.as_mut().unwrap().capture_output(
                                0,
                                output,
                                &event_queue.handle(),
                                (),
                            );
                        }
                        _ => {
                            unreachable!();
                        }
                    }
                }
            }

            event_queue
                .blocking_dispatch(self)
                .expect("Error running wayland capturer main loop");
        }
    }
}

// ==== Globals ====

impl Dispatch<WlOutput, GlobalsContext> for Capturer {
    fn event(
        state: &mut Self,
        output: &WlOutput,
        event: wl_output::Event,
        ctx: &GlobalsContext,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_output::Event::Description { description }
                if description.contains(&ctx.desired_output) =>
            {
                if state.output.is_none() {
                    log::debug!(
                        "Using output '{}' for config '{}'",
                        description,
                        ctx.desired_output,
                    );
                    state.output = Some(output.clone());
                    state.output_global_id = ctx.global_id;
                } else {
                    log::error!("Cannot use output '{}' for config '{}' because another output was already matched with it, skipping this output.", description, ctx.desired_output);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<WlRegistry, GlobalsContext> for Capturer {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        ctx: &GlobalsContext,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
                ..
            } => {
                match &interface[..] {
                    "wl_output" => {
                        registry.bind::<WlOutput, _, _>(
                            name,
                            version,
                            qh,
                            GlobalsContext {
                                global_id: Some(name),
                                desired_output: ctx.desired_output.clone(),
                            },
                        );
                    }
                    "zwlr_export_dmabuf_manager_v1" => {
                        log::debug!("Detected support for wlr-export-dmabuf-unstable-v1 protocol");
                        state.dmabuf_manager = Some(
                            registry.bind::<ZwlrExportDmabufManagerV1, _, _>(name, version, qh, ()),
                        );
                    }
                    "zwp_linux_dmabuf_v1" => {
                        state.dmabuf =
                            Some(registry.bind::<ZwpLinuxDmabufV1, _, _>(name, version, qh, ()));
                    }
                    "zwlr_screencopy_manager_v1" => {
                        log::debug!("Detected support for wlr-screencopy-unstable-v1 protocol");
                        state.screencopy_manager = Some(
                            registry.bind::<ZwlrScreencopyManagerV1, _, _>(name, version, qh, ()),
                        );
                    }
                    _ => {}
                };
            }
            wl_registry::Event::GlobalRemove { name } => {
                if Some(name) == state.output_global_id {
                    log::debug!("Disconnected screen {}", ctx.desired_output);
                    state.output = None;
                    state.output_global_id = None;
                }
            }
            _ => {}
        }
    }
}

// ==== wlr-export-dmabuf-unstable-v1 protocol ====

impl Dispatch<ZwlrExportDmabufManagerV1, ()> for Capturer {
    fn event(
        _: &mut Self,
        _: &ZwlrExportDmabufManagerV1,
        _: zwlr_export_dmabuf_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrExportDmabufFrameV1, ()> for Capturer {
    fn event(
        state: &mut Self,
        frame: &ZwlrExportDmabufFrameV1,
        event: zwlr_export_dmabuf_frame_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_export_dmabuf_frame_v1::Event::Frame {
                width,
                height,
                num_objects,
                format,
                ..
            } => {
                state.pending_frame = Some(Object::new(width, height, num_objects, format));
            }

            zwlr_export_dmabuf_frame_v1::Event::Object {
                index, fd, size, ..
            } => {
                state
                    .pending_frame
                    .as_mut()
                    .unwrap()
                    .set_object(index, fd, size);
            }

            zwlr_export_dmabuf_frame_v1::Event::Ready { .. } => {
                let luma = state
                    .vulkan
                    .as_ref()
                    .unwrap()
                    .luma_percent(&state.pending_frame.take().unwrap())
                    .expect("Unable to compute luma percent");

                state.controller.as_mut().unwrap().adjust(luma);

                frame.destroy();

                thread::sleep(DELAY_SUCCESS);
                state.is_processing_frame = false;
            }

            zwlr_export_dmabuf_frame_v1::Event::Cancel { reason } => {
                frame.destroy();

                log::error!("Frame was cancelled, reason: {reason:?}");
                thread::sleep(DELAY_FAILURE);
                state.is_processing_frame = false;
            }

            _ => unreachable!(),
        }
    }
}

// ==== wlr-screencopy-unstable-v1 protocol ====

impl Dispatch<ZwpLinuxDmabufV1, ()> for Capturer {
    fn event(
        _: &mut Self,
        _: &ZwpLinuxDmabufV1,
        _: zwp_linux_dmabuf_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpLinuxBufferParamsV1, ()> for Capturer {
    event_created_child!(Capturer, ZwpLinuxBufferParamsV1, [
        0 => (WlBuffer, ()),
    ]);

    fn event(
        state: &mut Self,
        _: &ZwpLinuxBufferParamsV1,
        event: zwp_linux_buffer_params_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwp_linux_buffer_params_v1::Event::Created { buffer } => {
                if let Some(screencopy_frame) = state.screencopy_frame.take() {
                    screencopy_frame.copy(&buffer);
                }
                state.wl_buffer = Some(buffer);
                if let Some(dmabuf_params) = state.dmabuf_params.take() {
                    dmabuf_params.destroy();
                }
            }
            zwp_linux_buffer_params_v1::Event::Failed => {
                log::error!("Failed creating WlBuffer");
                if let Some(screencopy_frame) = state.screencopy_frame.take() {
                    screencopy_frame.destroy();
                }
                if let Some(dmabuf_params) = state.dmabuf_params.take() {
                    dmabuf_params.destroy();
                }

                thread::sleep(DELAY_FAILURE);
                state.is_processing_frame = false;
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrScreencopyManagerV1, ()> for Capturer {
    fn event(
        _: &mut Self,
        _: &ZwlrScreencopyManagerV1,
        _: zwlr_screencopy_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlBuffer, ()> for Capturer {
    fn event(
        _: &mut Self,
        _: &WlBuffer,
        _: wl_buffer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrScreencopyFrameV1, ()> for Capturer {
    fn event(
        state: &mut Self,
        frame: &ZwlrScreencopyFrameV1,
        event: zwlr_screencopy_frame_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_screencopy_frame_v1::Event::LinuxDmabuf {
                width,
                height,
                format,
            } => {
                let pending_frame = Object::new(width, height, 1, format);
                let dmabuf_params = state.dmabuf.as_ref().unwrap().create_params(qh, ());
                let (fd, offset, stride, modifier) = state
                    .vulkan
                    .as_ref()
                    .unwrap()
                    .init_exportable_frame_image(&pending_frame)
                    .expect("Unable to init exportable frame image");

                let fd = unsafe { BorrowedFd::borrow_raw(fd) };

                dmabuf_params.add(
                    fd,
                    0,
                    offset as u32,
                    stride as u32,
                    (modifier >> 32) as u32,
                    (modifier & 0xFFFFFFFF) as u32,
                );

                dmabuf_params.create(width as i32, height as i32, format, Flags::empty());

                state.screencopy_frame = Some(frame.clone());
                state.pending_frame = Some(pending_frame);
                state.dmabuf_params = Some(dmabuf_params);
            }

            zwlr_screencopy_frame_v1::Event::Ready { .. } => {
                let luma = state
                    .vulkan
                    .as_ref()
                    .unwrap()
                    .luma_percent(&state.pending_frame.take().unwrap())
                    .expect("Unable to compute luma percent");

                state.controller.as_mut().unwrap().adjust(luma);

                frame.destroy();
                if let Some(buffer) = state.wl_buffer.take() {
                    buffer.destroy()
                }

                thread::sleep(DELAY_SUCCESS);
                state.is_processing_frame = false;
            }

            zwlr_screencopy_frame_v1::Event::Failed {} => {
                log::error!("Frame copy failed");
                frame.destroy();
                if let Some(buffer) = state.wl_buffer.take() {
                    buffer.destroy()
                }

                thread::sleep(DELAY_FAILURE);
                state.is_processing_frame = false;
            }

            _ => {}
        }
    }
}
