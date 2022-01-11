use crate::predictor::Controller;

pub mod none;
pub mod wlroots;

pub trait Capturer {
    fn run(&self, output_name: &str, controller: Controller);
}
