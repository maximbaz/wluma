use crate::frame::object::Object;
use crate::frame::vulkan::Vulkan;
use crate::predictor::Controller;
use std::thread;
use std::time::Duration;
use wayland_client::protocol::wl_output;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::protocol::wl_registry;
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::Connection;
use wayland_client::Dispatch;
use wayland_client::QueueHandle;
use wayland_protocols_wlr::export_dmabuf::v1::client::zwlr_export_dmabuf_frame_v1::Event;
use wayland_protocols_wlr::export_dmabuf::v1::client::zwlr_export_dmabuf_frame_v1::ZwlrExportDmabufFrameV1;
use wayland_protocols_wlr::export_dmabuf::v1::client::zwlr_export_dmabuf_manager_v1;
use wayland_protocols_wlr::export_dmabuf::v1::client::zwlr_export_dmabuf_manager_v1::ZwlrExportDmabufManagerV1;

const DELAY_SUCCESS: Duration = Duration::from_millis(100);
const DELAY_FAILURE: Duration = Duration::from_millis(1000);

#[derive(Default)]
pub struct Capturer {
    vulkan: Option<Vulkan>,
    output: Option<WlOutput>,
    output_global_id: Option<u32>,
    dmabuf_manager: Option<ZwlrExportDmabufManagerV1>,
    pending_frame: Option<Object>,
    controller: Option<Controller>,
}

#[derive(Clone)]
struct GlobalsContext {
    global_id: Option<u32>,
    desired_output: String,
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

        if self.dmabuf_manager.is_none() {
            panic!("Unable to initialize ZwlrExportDmabufManagerV1 instance");
        }

        self.vulkan = Some(Vulkan::new().expect("Unable to initialize Vulkan"));
        self.controller = Some(controller);

        loop {
            if let Some(output) = self.output.as_mut() {
                self.dmabuf_manager.as_mut().unwrap().capture_output(
                    0,
                    output,
                    &event_queue.handle(),
                    (),
                );
            }

            event_queue
                .blocking_dispatch(self)
                .expect("Error running wlr-export-dmabuf-unstable-v1 capturer main loop");
        }
    }
}

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

impl Dispatch<ZwlrExportDmabufFrameV1, ()> for Capturer {
    fn event(
        state: &mut Self,
        frame: &ZwlrExportDmabufFrameV1,
        event: Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            Event::Frame {
                width,
                height,
                num_objects,
                format,
                ..
            } => {
                state.pending_frame = Some(Object::new(width, height, num_objects, format));
            }

            Event::Object {
                index, fd, size, ..
            } => {
                state
                    .pending_frame
                    .as_mut()
                    .unwrap()
                    .set_object(index, fd, size);
            }

            Event::Ready { .. } => {
                let luma = state
                    .vulkan
                    .as_ref()
                    .unwrap()
                    .luma_percent(state.pending_frame.as_ref().unwrap())
                    .expect("Unable to compute luma percent");

                state.controller.as_mut().unwrap().adjust(luma);

                frame.destroy();
                state.pending_frame = None;

                thread::sleep(DELAY_SUCCESS);
            }

            Event::Cancel { reason } => {
                frame.destroy();
                state.pending_frame = None;

                log::error!("Frame was cancelled, reason: {reason:?}");
                thread::sleep(DELAY_FAILURE);
            }

            _ => unreachable!(),
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
                        state.dmabuf_manager = Some(
                            registry.bind::<ZwlrExportDmabufManagerV1, _, _>(name, version, qh, ()),
                        );
                    }
                    _ => {}
                };
            }
            wl_registry::Event::GlobalRemove { name } => {
                if Some(name) == state.output_global_id {
                    log::info!("Disconnected screen {}", ctx.desired_output);
                    state.output = None;
                    state.output_global_id = None;
                }
            }
            _ => {}
        }
    }
}
