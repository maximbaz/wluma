pub mod none;
pub mod wlroots;

pub trait Capturer: Send {
    fn run(&mut self);
}
