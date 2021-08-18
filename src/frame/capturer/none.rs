use crate::controller::Controller;
use std::{thread, time::Duration};

#[derive(Default)]
pub struct Capturer {}

impl super::Capturer for Capturer {
    fn run(&self, mut controller: Controller) {
        loop {
            controller.adjust(None).expect("TODO");
            thread::sleep(Duration::from_secs(1));
        }
    }
}
