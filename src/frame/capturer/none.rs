use crate::predictor::Controller;
use std::error::Error;
use std::{thread, time::Duration};

pub struct Capturer {
    controller: Controller,
}

impl Capturer {
    pub fn new(controller: Controller) -> Result<Self, Box<dyn Error>> {
        Ok(Self { controller })
    }
}

impl super::Capturer for Capturer {
    fn run(&mut self) {
        loop {
            self.controller.adjust(0);
            thread::sleep(Duration::from_millis(200));
        }
    }
}
