use std::error::Error;

mod backlight;
mod controller;
mod ddcutil;

pub use backlight::Backlight;
pub use controller::Controller;
pub use ddcutil::DdcUtil;

#[cfg(test)]
use mockall::*;
#[cfg_attr(test, automock)]
pub trait Brightness {
    fn get(&self) -> Result<u64, Box<dyn Error>>;
    fn set(&self, value: u64) -> Result<u64, Box<dyn Error>>;
}
