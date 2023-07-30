use crate::device_file::{read, write};
use inotify::{Inotify, WatchMask};
use std::error::Error;
use std::fs;
use std::fs::{File, OpenOptions};
use std::io::ErrorKind;
use std::path::Path;
use dbus::{self, blocking::Connection, Message, channel::Sender};

pub struct Backlight {
    file: File,
    min_brightness: u64,
    max_brightness: u64,
    inotify: Inotify,
    current: Option<u64>,
    dbus_conn: Option<Connection>,
    dbus_msg: Option<Message>,
}

impl Backlight {
    pub fn new(path: &str, min_brightness: u64) -> Result<Self, Box<dyn Error>> {
        let id = Path::new(path).file_name().unwrap().to_string_lossy().to_string();
        let dbus_conn = Connection::new_system().ok();
        let dbus_msg = Message::new_method_call(
            "org.freedesktop.login1",
            "/org/freedesktop/login1/session/auto",
            "org.freedesktop.login1.Session",
            "SetBrightness",
        ).ok().map(|m| m.append2("backlight", &id));

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
            dbus_conn,
            dbus_msg,
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

        if let (Some(conn), Some(msg)) = (&self.dbus_conn, &self.dbus_msg) {
            let msg = msg
                .duplicate()?
                .append1(value as u32);

            if conn.send(msg).is_err() {
                write(&mut self.file, value as f64)?;
            }
        } else {
            write(&mut self.file, value as f64)?;
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
