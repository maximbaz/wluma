use chrono::{Local, Timelike};
use std::collections::HashMap;
use std::error::Error;

pub struct Als {
    thresholds: HashMap<u64, String>,
}

impl Als {
    pub fn new(thresholds: HashMap<u64, String>) -> Self {
        Self { thresholds }
    }
}

impl super::Als for Als {
    fn get(&self) -> Result<String, Box<dyn Error>> {
        let raw = Local::now().hour() as u64;
        let profile = super::find_profile(raw, &self.thresholds);

        log::trace!("ALS (time): {} ({})", profile, raw);
        Ok(profile)
    }
}
