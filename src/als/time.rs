use anyhow::Result;
use chrono::{Local, Timelike};
use std::collections::HashMap;

pub struct Als {
    thresholds: HashMap<u64, String>,
}

impl Als {
    pub fn new(thresholds: HashMap<u64, String>) -> Self {
        Self { thresholds }
    }

    pub async fn get(&self) -> Result<String> {
        let raw = Local::now().hour() as u64;
        let profile = super::find_profile(raw, &self.thresholds);

        log::trace!("ALS (time): {} ({})", profile, raw);
        Ok(profile)
    }
}
