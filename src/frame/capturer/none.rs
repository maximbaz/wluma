use crate::predictor::Controller;
use std::{thread, time::Duration};

#[derive(Default)]
pub struct Capturer {}

impl super::Capturer for Capturer {
    fn run(&self, mut controller: Controller) {
        loop {
            controller.adjust(None);
            thread::sleep(Duration::from_secs(1));
        }
    }
}
