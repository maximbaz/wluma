use chrono::{Local, Timelike};
use std::collections::HashMap;
use std::error::Error;

pub struct Als {
    hour_to_lux: HashMap<u32, u64>,
    maximum_value: u64,
}

impl Als {
    pub fn new(hour_to_lux: &HashMap<String, u64>) -> Self {
        let hour_to_lux: HashMap<u32, u64> = (0..24)
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
            .collect();

        let maximum_value = *hour_to_lux
            .values()
            .max()
            .expect("Unable to find ALS (time) maximum value");

        Self {
            hour_to_lux,
            maximum_value,
        }
    }
}

impl super::Als for Als {
    fn get(&self) -> Result<u64, Box<dyn Error>> {
        let smooth = *self
            .hour_to_lux
            .get(&Local::now().hour())
            .ok_or("Unable to find ALS (time) value for the current hour")?;

        let percent = super::to_percent(smooth, self.maximum_value)?;

        log::trace!("ALS (time): {:>3}%  <--  {:>3} (smooth)", percent, smooth);

        Ok(percent)
    }
}
