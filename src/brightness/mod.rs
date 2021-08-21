use mockall::*;
use std::error::Error;

mod backlight;
mod controller;

pub use backlight::Backlight;
pub use controller::Controller;

#[automock]
pub trait Brightness {
    fn get(&self) -> Result<u64, Box<dyn Error>>;
    fn set(&self, value: u64) -> Result<u64, Box<dyn Error>>;
}
