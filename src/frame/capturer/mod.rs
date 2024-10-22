use crate::predictor::Controller;

pub mod none;
pub mod wlr_export_dmabuf_unstable_v1;

pub trait Capturer {
    fn run(&mut self, output_name: &str, controller: Controller);
}
