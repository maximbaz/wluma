use chrono::{Local, Timelike};
use std::collections::HashMap;
use std::error::Error;

pub struct Als {
    hour_to_lux: HashMap<u32, u64>,
}

impl Als {
    pub fn new(hour_to_lux: &HashMap<String, u64>) -> Self {
        Self {
            hour_to_lux: (0..24)
                .into_iter()
                .fold(Vec::<(u32, u64)>::new(), |mut acc, hour| {
                    let lux = hour_to_lux
                        .get(&hour.to_string())
                        .copied()
                        .unwrap_or_else(|| acc.last().map(|&v| v.1).unwrap_or(0));
                    acc.push((hour, lux));
                    acc
                })
                .into_iter()
                .collect(),
        }
    }
}

impl super::Als for Als {
    fn get_raw(&self) -> Result<u64, Box<dyn Error>> {
        Ok(*self
            .hour_to_lux
            .get(&Local::now().hour())
            .ok_or("Unable to find ALS value for the current hour")?)
    }
}
