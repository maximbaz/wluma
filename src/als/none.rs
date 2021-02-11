use std::error::Error;

#[derive(Default)]
pub struct Als {}

impl super::Als for Als {
    fn get_raw(&self) -> Result<f64, Box<dyn Error>> {
        Ok(0.0)
    }
}
