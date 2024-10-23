use crate::predictor::Controller;

pub mod none;
pub mod wayland;

pub trait Capturer {
    fn run(&mut self, output_name: &str, controller: Controller);
}
