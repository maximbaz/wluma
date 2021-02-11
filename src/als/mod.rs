use std::error::Error;

pub mod iio;
pub mod none;
pub mod time;

pub trait Als {
    fn get_raw(&self) -> Result<f64, Box<dyn Error>>;

    fn get(&self) -> Result<u64, Box<dyn Error>> {
        Ok(self.get_raw()?.log10() as u64)
    }
}
