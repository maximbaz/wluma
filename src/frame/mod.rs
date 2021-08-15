use crate::controller::Controller;

pub mod none;
pub mod object;
pub mod wlroots;

pub trait Capturer {
    fn run(&self, controller: Controller);
}
