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
use wayland_client::WEnum;
use wayland_protocols_wlr::export_dmabuf::v1::client::zwlr_export_dmabuf_frame_v1::CancelReason;
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
    dmabuf_manager: Option<ZwlrExportDmabufManagerV1>,
    pending_frame: Option<Object>,
    controller: Option<Controller>,
}

#[derive(Clone)]
struct GlobalsContext {
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

        if self.output.is_none() {
            panic!("Unable to find output that matches config '{output_name}'");
        }

        if self.dmabuf_manager.is_none() {
            panic!("Unable to initialize ZwlrExportDmabufManagerV1 instance");
        }

        self.vulkan = Some(Vulkan::new().expect("Unable to initialize Vulkan"));
        self.controller = Some(controller);

        loop {
            self.dmabuf_manager.as_mut().unwrap().capture_output(
                0,
                self.output.as_mut().unwrap(),
                &event_queue.handle(),
                (),
            );

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
                log::debug!(
                    "Using output '{}' for config '{}'",
                    description,
                    ctx.desired_output,
                );
                state.output = Some(output.clone());
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

                match reason {
                    WEnum::Value(CancelReason::Permanent) => {
                        panic!("Frame was cancelled due to a permanent error. If you just disconnected screen, this is not implemented yet.");
                    }
                    _ => {
                        log::error!(
                            "Frame was cancelled due to a temporary error, will try again."
                        );
                        thread::sleep(DELAY_FAILURE);
                    }
                }
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
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
            ..
        } = event
        {
            match &interface[..] {
                "wl_output" => {
                    registry.bind::<WlOutput, _, _>(name, version, qh, ctx.clone());
                }
                "zwlr_export_dmabuf_manager_v1" => {
                    state.dmabuf_manager = Some(registry.bind::<ZwlrExportDmabufManagerV1, _, _>(
                        name,
                        version,
                        qh,
                        (),
                    ));
                }
                _ => {}
            };
        }
    }
}
