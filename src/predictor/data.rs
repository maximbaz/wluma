use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs::{create_dir_all, File, OpenOptions};
use std::path::PathBuf;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone, Default)]
pub struct Data {
    pub entries: Vec<Entry>,
}

#[derive(Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Clone)]
pub struct Entry {
    pub lux: u64,
    pub luma: Option<u8>,
    pub brightness: u64,
}

impl Data {
    pub fn load() -> Result<Data, Box<dyn Error>> {
        Ok(serde_yaml::from_reader(Self::read_file()?)?)
    }

    pub fn save(&self) -> Result<(), Box<dyn Error>> {
        Ok(serde_yaml::to_writer(Self::write_file()?, self)?)
    }

    fn read_file() -> Result<File, Box<dyn Error>> {
        Ok(OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(Self::path()?)?)
    }

    fn write_file() -> Result<File, Box<dyn Error>> {
        Ok(OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(Self::path()?)?)
    }

    fn path() -> Result<PathBuf, Box<dyn Error>> {
        let datadir = dirs::data_dir()
            .ok_or("Unable to get data dir")?
            .join("wluma");
        create_dir_all(datadir.clone())?;
        Ok(datadir.join("data.yaml"))
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
