use crate::frame::capturer::Adjustable;
use std::{thread, time::Duration};

#[derive(Default)]
pub struct Capturer {}

impl super::Capturer for Capturer {
    fn run(&mut self, _output_name: &str, mut controller: Box<dyn Adjustable>) {
        loop {
            controller.adjust(0);
            thread::sleep(Duration::from_millis(200));
        }
    }
}
