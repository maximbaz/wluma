use crate::frame::object::Object;
use std::error::Error;

pub mod vulkan;

pub trait Processor {
    fn luma_percent(&self, frame: &Object) -> Result<u8, Box<dyn Error>>;
}
