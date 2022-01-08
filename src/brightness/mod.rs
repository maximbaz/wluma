use mockall::*;
use std::error::Error;

mod backlight;
mod controller;
mod ddcutil;

pub use backlight::Backlight;
pub use controller::Controller;
pub use ddcutil::DdcUtil;

#[automock]
pub trait Brightness {
    fn get(&mut self) -> Result<u64, Box<dyn Error>>;
    fn set(&mut self, value: u64) -> Result<u64, Box<dyn Error>>;
}
