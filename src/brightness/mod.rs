use mockall::*;
use std::error::Error;

mod backlight;

pub use backlight::Backlight;

#[automock]
pub trait Brightness {
    fn get(&self) -> Result<u64, Box<dyn Error>>;
    fn set(&self, value: u64) -> Result<(), Box<dyn Error>>;
}
