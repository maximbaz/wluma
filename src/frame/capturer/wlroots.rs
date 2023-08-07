use crate::frame::object::Object;
use crate::frame::vulkan::Vulkan;
use crate::predictor::Controller;
use itertools::Itertools;
use smithay_client_toolkit::delegate_simple;
use smithay_client_toolkit::{
    delegate_output, delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
};
use std::error::Error;
use std::os::fd::AsRawFd;
use std::{thread, time::Duration};
use wayland_client::globals::GlobalListContents;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_output, wl_registry},
    Connection, Dispatch, EventQueue,
};
use wayland_client::{QueueHandle, WEnum};
use wayland_protocols_wlr::export_dmabuf::v1::client::{
    zwlr_export_dmabuf_frame_v1, zwlr_export_dmabuf_manager_v1,
};

const DELAY_SUCCESS: Duration = Duration::from_millis(100);
const DELAY_FAILURE: Duration = Duration::from_millis(1000);

pub struct Capturer {
    vulkan: Vulkan,
    connection: Connection,
    output: wl_output::WlOutput,
    controller: Controller,
    pending_frame: Option<Object>,
}

impl Capturer {
    pub fn new(output_name: &str, controller: Controller) -> Result<Self, Box<dyn Error>> {
        let connection = Connection::connect_to_env().expect("Unable to connect to Wayland");

        Ok(Self {
            vulkan: Vulkan::new().expect("Unable to initialize Vulkan"),
            output: find_output(&connection, output_name)?,
            connection,
            controller,
            pending_frame: None,
        })
    }
}

impl super::Capturer for Capturer {
    fn run(&mut self) {
        let (mut event_queue, dmabuf_manager) =
            init_dmabuf(&self.connection).expect("Unable to init dmabuf_manager");

        loop {
            if self.pending_frame.is_none() {
                self.pending_frame = Some(Object::default());
                dmabuf_manager.capture_output(0, &self.output, &event_queue.handle(), ());
            }

            event_queue
                .roundtrip(self)
                .expect("Error running wlroots capturer main loop");
        }
    }
}

fn find_output(connection: &Connection, output_name: &str) -> Result<WlOutput, Box<dyn Error>> {
    let (globals, mut event_queue) = registry_queue_init(connection)?;

    let mut list_outputs = ListOutputs {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &event_queue.handle()),
    };

    event_queue.roundtrip(&mut list_outputs)?;

    let mut outputs = list_outputs
        .output_state
        .outputs()
        .filter(|o| {
            list_outputs
                .output_state
                .info(o)
                .and_then(|i| {
                    i.description.map(|d| {
                        d.contains(output_name)
                            .then(|| {
                                log::debug!(
                                    "Discovered output '{d}' that matches config '{output_name}'"
                                )
                            })
                            .is_some()
                    })
                })
                .unwrap_or(false)
        })
        .collect_vec();

    match outputs.len() {
        0 => panic!("Unable to find output that matches config '{output_name}'"),
        1 => Ok(outputs.pop().unwrap()),
        _ => panic!("More than one output matches config '{output_name}', this is not supported!"),
    }
}

fn init_dmabuf(
    connection: &Connection,
) -> Result<
    (
        EventQueue<Capturer>,
        zwlr_export_dmabuf_manager_v1::ZwlrExportDmabufManagerV1,
    ),
    Box<dyn Error>,
> {
    let (global_list, event_queue) = registry_queue_init::<Capturer>(connection)?;
    let dmabuf_manager: zwlr_export_dmabuf_manager_v1::ZwlrExportDmabufManagerV1 =
        global_list.bind(&event_queue.handle(), 1..=1, ())?;
    Ok((event_queue, dmabuf_manager))
}

impl Dispatch<zwlr_export_dmabuf_frame_v1::ZwlrExportDmabufFrameV1, ()> for Capturer {
    fn event(
        state: &mut Self,
        frame: &zwlr_export_dmabuf_frame_v1::ZwlrExportDmabufFrameV1,
        event: zwlr_export_dmabuf_frame_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let pending_frame = state
            .pending_frame
            .as_mut()
            .expect("Unable to access pending frame");

        match event {
            zwlr_export_dmabuf_frame_v1::Event::Frame {
                width,
                height,
                num_objects,
                ..
            } => {
                pending_frame.set_metadata(width, height, num_objects);
            }

            zwlr_export_dmabuf_frame_v1::Event::Object {
                index, fd, size, ..
            } => {
                pending_frame.set_object(index, fd.as_raw_fd(), size);
            }

            zwlr_export_dmabuf_frame_v1::Event::Ready { .. } => {
                let luma = state
                    .vulkan
                    .luma_percent(pending_frame)
                    .expect("Unable to compute luma percent");

                state.controller.adjust(luma);

                frame.destroy();

                thread::sleep(DELAY_SUCCESS);
                state.pending_frame = None;
            }

            zwlr_export_dmabuf_frame_v1::Event::Cancel { reason } => {
                frame.destroy();

                match reason {
                    WEnum::Value(reason)
                        if reason == zwlr_export_dmabuf_frame_v1::CancelReason::Permanent =>
                    {
                        panic!("Frame was cancelled due to a permanent error. If you just disconnected screen, this is not implemented yet.");
                    }
                    _ => {
                        log::error!(
                            "Frame was cancelled due to a temporary error, will try again."
                        );
                        thread::sleep(DELAY_FAILURE);
                        state.pending_frame = None;
                    }
                }
            }

            _ => unreachable!(),
        }
    }
}

struct ListOutputs {
    registry_state: RegistryState,
    output_state: OutputState,
}

impl OutputHandler for ListOutputs {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

delegate_output!(ListOutputs);

delegate_registry!(ListOutputs);

delegate_simple!(
    Capturer,
    zwlr_export_dmabuf_manager_v1::ZwlrExportDmabufManagerV1,
    1
);

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Capturer {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl ProvidesRegistryState for ListOutputs {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers! {
        OutputState,
    }
}
