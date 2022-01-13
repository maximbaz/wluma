use std::error::Error;

#[derive(Default)]
pub struct Als {}

impl super::Als for Als {
    fn get(&self) -> Result<String, Box<dyn Error>> {
        Ok("none".to_string())
    }
}
