use crate::frame::object::Object;
use crate::frame::processor::Processor;
use crate::predictor::Controller;
use std::{cell::RefCell, rc::Rc, thread, time::Duration};
use wayland_client::protocol::wl_output;
use wayland_client::{
    protocol::wl_registry::WlRegistry, Display as WaylandDisplay, EventQueue, GlobalManager, Main,
};
use wayland_protocols::wlr::unstable::export_dmabuf::v1::client::{
    zwlr_export_dmabuf_frame_v1::{CancelReason, Event as FrameEvent},
    zwlr_export_dmabuf_manager_v1::ZwlrExportDmabufManagerV1,
};

const DELAY_SUCCESS: Duration = Duration::from_millis(100);
const DELAY_FAILURE: Duration = Duration::from_millis(1000);

#[derive(Clone)]
pub struct Capturer {
    event_queue: Rc<RefCell<EventQueue>>,
    globals: GlobalManager,
    dmabuf_manager: Main<ZwlrExportDmabufManagerV1>,
    processor: Rc<dyn Processor>,
    registry: Main<WlRegistry>,
}

impl super::Capturer for Capturer {
    fn run(&self, output_name: &str, controller: Controller) {
        let controller = Rc::new(RefCell::new(controller));
        self.globals
            .list()
            .iter()
            .filter(|(_, interface, _)| interface == "wl_output")
            .for_each(|(id, _, _)| {
                let output = Rc::new(self.registry.bind::<wl_output::WlOutput>(1, *id));
                let capturer = Rc::new(self.clone());
                let controller = controller.clone();
                let desired_output = output_name.to_string();
                output.clone().quick_assign(move |_, event, _| match event {
                    wl_output::Event::Geometry { make, model, .. } => {
                        let actual_output = format!("{} ({})", make, model);
                        if actual_output == desired_output {
                            capturer
                                .clone()
                                .capture_frame(controller.clone(), output.clone())
                        }
                    }
                    _ => {}
                })
            });

        loop {
            self.event_queue
                .borrow_mut()
                .dispatch(&mut (), |_, _, _| {})
                .unwrap();
        }
    }
}

impl Capturer {
    pub fn new(processor: Box<dyn Processor>) -> Self {
        let display = WaylandDisplay::connect_to_env().unwrap();
        let mut event_queue = display.create_event_queue();
        let attached_display = display.attach(event_queue.token());
        let registry = attached_display.get_registry();
        let globals = GlobalManager::new(&attached_display);

        event_queue.sync_roundtrip(&mut (), |_, _, _| {}).unwrap();

        let dmabuf_manager = globals
            .instantiate_exact::<ZwlrExportDmabufManagerV1>(1)
            .expect("unable to init export_dmabuf_manager");

        Self {
            event_queue: Rc::new(RefCell::new(event_queue)),
            globals,
            registry,
            dmabuf_manager,
            processor: processor.into(),
        }
    }

    fn capture_frame(
        self: Rc<Self>,
        controller: Rc<RefCell<Controller>>,
        output: Rc<Main<wl_output::WlOutput>>,
    ) {
        let mut frame = Object::default();

        self.dmabuf_manager
            .capture_output(0, &output)
            .quick_assign(move |data, event, _| match event {
                FrameEvent::Frame {
                    width,
                    height,
                    num_objects,
                    ..
                } => {
                    frame.set_metadata(width, height, num_objects);
                }

                FrameEvent::Object {
                    index, fd, size, ..
                } => {
                    frame.set_object(index, fd, size);
                }

                FrameEvent::Ready { .. } => {
                    let luma = self
                        .processor
                        .luma_percent(&frame)
                        .expect("Unable to compute luma percent");

                    controller.borrow_mut().adjust(Some(luma));

                    data.destroy();

                    thread::sleep(DELAY_SUCCESS);
                    self.clone().capture_frame(controller.clone(), output.clone());
                }

                FrameEvent::Cancel { reason } => {
                    data.destroy();

                    if reason == CancelReason::Permanent {
                        panic!("Frame was cancelled due to a permanent error. If you just disconnected screen, this is not implemented yet.");
                    } else {
                        log::error!("Frame was cancelled due to a temporary error, will try again.");
                        thread::sleep(DELAY_FAILURE);
                        self.clone().capture_frame(controller.clone(), output.clone());
                    }
                }

                _ => unreachable!(),
            });
    }
}
