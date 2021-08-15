use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs::{File, OpenOptions};

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone, Default)]
pub struct Data {
    pub entries: Vec<Entry>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Entry {
    pub lux: u64,
    pub luma: Option<u8>,
    pub brightness: u64,
}

impl Data {
    pub fn load() -> Result<Data, Box<dyn Error>> {
        Ok(serde_yaml::from_reader(Self::file()?)?)
    }

    pub fn save(&self) -> Result<(), Box<dyn Error>> {
        Ok(serde_yaml::to_writer(Self::file()?, self)?)
    }

    fn file() -> Result<File, Box<dyn Error>> {
        let xdg_dirs = xdg::BaseDirectories::with_prefix("wluma")?;
        let path = xdg_dirs.place_data_file("data.yaml")?;

        Ok(OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(path)?)
    }
}

impl Entry {
    pub fn new(lux: u64, luma: Option<u8>, brightness: u64) -> Self {
        Self {
            lux,
            luma,
            brightness,
        }
    }
}
