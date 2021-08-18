use mockall::*;
use std::error::Error;

pub mod iio;
pub mod none;
pub mod time;

#[automock]
pub trait Als {
    fn get(&self) -> Result<u64, Box<dyn Error>>;
}
