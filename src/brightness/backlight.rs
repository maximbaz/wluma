use crate::device_file::{read, write};
use inotify::{Inotify, WatchMask};
use std::error::Error;
use std::fs;
use std::fs::{File, OpenOptions};
use std::io::ErrorKind;
use std::path::Path;

pub struct Backlight {
    file: File,
    min_brightness: u64,
    max_brightness: u64,
    inotify: Inotify,
    current: Option<u64>,
}

impl Backlight {
    pub fn new(path: &str, min_brightness: u64) -> Result<Self, Box<dyn Error>> {
        let brightness_path = Path::new(path).join("brightness");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&brightness_path)?;

        let max_brightness = fs::read_to_string(Path::new(path).join("max_brightness"))?
            .trim()
            .parse()?;

        let mut inotify = Inotify::init()?;
        inotify.add_watch(&brightness_path, WatchMask::MODIFY)?;

        let brightness_hw_changed_path = Path::new(path).join("brightness_hw_changed");
        if Path::new(&brightness_hw_changed_path).exists() {
            inotify.add_watch(&brightness_hw_changed_path, WatchMask::MODIFY)?;
        }

        Ok(Self {
            file,
            min_brightness,
            max_brightness,
            inotify,
            current: None,
        })
    }
}

impl super::Brightness for Backlight {
    fn get(&mut self) -> Result<u64, Box<dyn Error>> {
        let mut buffer = [0u8; 1024];
        match (self.inotify.read_events(&mut buffer), self.current) {
            (_, None) => {
                let value = read(&mut self.file)? as u64;
                self.current = Some(value);
                Ok(value)
            }
            (Ok(mut event), Some(value)) => {
                if event.next().is_some() {
                    let value = read(&mut self.file)? as u64;
                    self.current = Some(value);
                    return Ok(value);
                }
                Ok(value)
            }
            (Err(error), Some(value)) if error.kind() == ErrorKind::WouldBlock => Ok(value),
            (_, Some(value)) => Ok(value),
        }
    }

    fn set(&mut self, value: u64) -> Result<u64, Box<dyn Error>> {
        let mut buffer = [0u8; 1024];
        let value = value.max(self.min_brightness).min(self.max_brightness) as u64;
        self.current = Some(value);
        write(&mut self.file, value as f64)?;
        self.inotify.read_events(&mut buffer)?;
        Ok(value)
    }
}
