use crate::controller::Controller;
use crate::frame::object::Object;
use crate::frame::Capturer;
use crate::vulkan::Vulkan;
use std::{cell::RefCell, rc::Rc, thread, time::Duration};
use wayland_client::{
    protocol::wl_output::WlOutput, Display as WaylandDisplay, EventQueue, GlobalManager, Main,
};
use wayland_protocols::wlr::unstable::export_dmabuf::v1::client::{
    zwlr_export_dmabuf_frame_v1::{CancelReason, Event},
    zwlr_export_dmabuf_manager_v1::ZwlrExportDmabufManagerV1,
};

const DELAY_SUCCESS: Duration = Duration::from_millis(100);
const DELAY_FAILURE: Duration = Duration::from_millis(1000);

#[derive(Clone)]
pub struct Wlroots {
    event_queue: Rc<RefCell<EventQueue>>,
    output: Main<WlOutput>,
    dmabuf_manager: Main<ZwlrExportDmabufManagerV1>,
    vulkan: Rc<Vulkan>,
}

impl Capturer for Wlroots {
    fn run(&self, controller: Controller) {
        Rc::new(self.clone()).capture_frame(Rc::new(RefCell::new(controller)));

        loop {
            self.event_queue
                .borrow_mut()
                .dispatch(&mut (), |_, _, _| {})
                .unwrap();
        }
    }
}

impl Wlroots {
    pub fn default() -> Self {
        let display = WaylandDisplay::connect_to_env().unwrap();
        let mut event_queue = display.create_event_queue();
        let attached_display = display.attach(event_queue.token());
        let globals = GlobalManager::new(&attached_display);

        event_queue.sync_roundtrip(&mut (), |_, _, _| {}).unwrap();

        let output = globals
            .instantiate_exact::<WlOutput>(1)
            .expect("unable to init wayland output");

        let dmabuf_manager = globals
            .instantiate_exact::<ZwlrExportDmabufManagerV1>(1)
            .expect("unable to init export_dmabuf_manager");

        Self {
            event_queue: Rc::new(RefCell::new(event_queue)),
            output,
            dmabuf_manager,
            vulkan: Rc::new(Vulkan::new().expect("unable to init vulkan")),
        }
    }

    fn capture_frame(self: Rc<Self>, controller: Rc<RefCell<Controller>>) {
        let mut frame = Object::default();

        self.dmabuf_manager
            .capture_output(0, &self.output)
            .quick_assign(move |data, event, _| match event {
                Event::Frame {
                    width,
                    height,
                    num_objects,
                    ..
                } => {
                    frame.set_metadata(width, height, num_objects);
                }

                Event::Object {
                    index, fd, size, ..
                } => {
                    frame.set_object(index, fd, size);
                }

                Event::Ready { .. } => {
                    controller
                        .borrow_mut()
                        .adjust(self.vulkan.luma_percent(&frame).ok())
                        .expect("TODO");

                    data.destroy();

                    thread::sleep(DELAY_SUCCESS);
                    self.clone().capture_frame(controller.clone());
                }

                Event::Cancel { reason } => {
                    data.destroy();

                    if reason == CancelReason::Permanent {
                        eprintln!("Frame was cancelled due to a permanent error");
                    } else {
                        eprintln!("Frame was cancelled due to a temporary error, will try again");
                        thread::sleep(DELAY_FAILURE);
                        self.clone().capture_frame(controller.clone());
                    }
                }

                _ => unreachable!(),
            });
    }
}
