pub mod none;
pub mod wlroots;

pub trait Capturer {
    fn run(&mut self);
}
