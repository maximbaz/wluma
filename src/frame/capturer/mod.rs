pub mod none;
pub mod wayland;

pub trait Adjustable {
    fn adjust(&mut self, luma: u8);
}

pub trait Capturer {
    fn run(&mut self, output_name: &str, controller: Box<dyn Adjustable>);
}
