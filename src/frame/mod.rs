use crate::controller::BrightnessController;

pub mod none;
pub mod object;
pub mod wlroots;

pub trait Capturer {
    fn run(&self, controller: BrightnessController);
}
