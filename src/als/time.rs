use chrono::{Local, Timelike};
use std::collections::HashMap;
use std::error::Error;

pub struct Als {
    hour_to_lux: HashMap<u32, u32>,
}

impl Als {
    pub fn new(hour_to_lux: &HashMap<String, u32>) -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            hour_to_lux: hour_to_lux
                .iter()
                .map(|(key, &val)| key.parse().map(|k| (k, val)))
                .collect::<Result<_, _>>()?,
        })
    }
}

impl super::Als for Als {
    fn get_raw(&self) -> Result<f64, Box<dyn Error>> {
        Ok(*self
            .hour_to_lux
            .get(&Local::now().hour())
            .ok_or("Unable to find ALS value for the current hour")? as f64)
    }
}
