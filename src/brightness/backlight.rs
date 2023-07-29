use crate::device_file::{read, write};
use inotify::{Inotify, WatchMask};
use std::error::Error;
use std::fs;
use std::fs::{File, OpenOptions};
use std::io::ErrorKind;
use std::path::Path;
use dbus::{self, blocking::{BlockingSender, Connection}};

pub struct Backlight {
    file: File,
    min_brightness: u64,
    max_brightness: u64,
    inotify: Inotify,
    current: Option<u64>,
    id: Option<String>,
    class: Option<String>,
}

impl Backlight {
    pub fn new(path: &str, min_brightness: u64) -> Result<Self, Box<dyn Error>> {
        let id = Path::new(path).file_name().map(|s| s.to_string_lossy().to_string());
        let class = Path::new(path).parent().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().to_string());

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
            id,
            class
        })
    }
}

impl super::Brightness for Backlight {
    fn get(&mut self) -> Result<u64, Box<dyn Error>> {
        let update = |this: &mut Self| {
            let value = read(&mut this.file)? as u64;
            this.current = Some(value);
            Ok(value)
        };

        let mut buffer = [0u8; 1024];
        match (self.inotify.read_events(&mut buffer), self.current) {
            (_, None) => update(self),
            (Ok(mut events), Some(cached)) => {
                if events.next().is_some() {
                    update(self)
                } else {
                    Ok(cached)
                }
            }
            (Err(err), Some(cached)) if err.kind() == ErrorKind::WouldBlock => Ok(cached),
            (Err(err), _) => Err(err.into()),
        }
    }

    fn set(&mut self, value: u64) -> Result<u64, Box<dyn Error>> {
        let value = value.clamp(self.min_brightness, self.max_brightness);

        if write(&mut self.file, value as f64).is_err() {
            if let (Some(class), Some(id)) = (&self.class, &self.id) {
                let conn = Connection::new_system()?;
                let msg = dbus::Message::new_method_call(
                    "org.freedesktop.login1",
                    "/org/freedesktop/login1/session/auto",
                    "org.freedesktop.login1.Session",
                    "SetBrightness",
                )?
                    .append2(class, id)
                    .append1(value as u32);

                conn.send_with_reply_and_block(msg, std::time::Duration::from_secs(1))?;
            }
        }

        self.current = Some(value);

        // Consume file events to not trigger get() update
        let mut buffer = [0u8; 1024];
        match self.inotify.read_events(&mut buffer) {
            Err(err) if err.kind() == ErrorKind::WouldBlock => Ok(value),
            Err(err) => Err(err.into()),
            _ => Ok(value),
        }
    }
}
