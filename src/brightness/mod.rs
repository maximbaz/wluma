use std::error::Error;

#[cfg(test)]
use mockall::*;

mod backlight;
mod controller;
mod ddcutil;

pub use backlight::Backlight;
pub use controller::Controller;
pub use ddcutil::DdcUtil;

#[cfg_attr(test, automock)]
pub trait Brightness {
    fn get(&self) -> Result<u64, Box<dyn Error>>;
    fn set(&self, value: u64) -> Result<u64, Box<dyn Error>>;
}
