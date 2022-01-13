use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs::{create_dir_all, File, OpenOptions};
use std::path::PathBuf;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub struct Data {
    pub entries: Vec<Entry>,
    #[serde(skip)]
    pub output_name: String,
}

#[derive(Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Clone)]
pub struct Entry {
    pub lux: u64,
    pub luma: u8,
    pub brightness: u64,
}

impl Data {
    pub fn new(output_name: &str) -> Self {
        Self {
            output_name: output_name.to_string(),
            entries: vec![],
        }
    }

    pub fn load(output_name: &str) -> Self {
        Self::path(output_name)
            .ok()
            .and_then(|path| Self::read_file(path).ok())
            .and_then(|file| serde_yaml::from_reader(file).ok())
            .unwrap_or_else(|| Self::new(output_name))
    }

    pub fn save(&self) -> Result<(), Box<dyn Error>> {
        Ok(serde_yaml::to_writer(self.write_file()?, self)?)
    }

    fn read_file(path: PathBuf) -> Result<File, Box<dyn Error>> {
        Ok(OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(path)?)
    }

    fn write_file(&self) -> Result<File, Box<dyn Error>> {
        let path = Self::path(&self.output_name).unwrap();
        Ok(OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?)
    }

    fn path(output_name: &str) -> Result<PathBuf, Box<dyn Error>> {
        let filename = format!("{:}.yaml", output_name);
        let datadir = dirs::data_dir()
            .ok_or("Unable to get data dir")?
            .join("wluma");
        create_dir_all(datadir.clone())?;
        Ok(datadir.join(filename))
    }
}

impl Entry {
    pub fn new(lux: u64, luma: u8, brightness: u64) -> Self {
        Self {
            lux,
            luma,
            brightness,
        }
    }
}
