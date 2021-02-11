use crate::controller::BrightnessController;
use std::{thread, time::Duration};

#[derive(Default)]
pub struct Capturer {}

impl super::Capturer for Capturer {
    fn run(&self, mut controller: BrightnessController) {
        loop {
            controller.adjust(None).expect("TODO");
            thread::sleep(Duration::from_secs(1));
        }
    }
}
