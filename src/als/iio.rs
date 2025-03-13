use crate::device_file::read;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use SensorType::*;

enum SensorType {
    Illuminance {
        value: Mutex<File>,
        scale: f64,
        offset: f64,
    },
    Intensity {
        r: Mutex<File>,
        g: Mutex<File>,
        b: Mutex<File>,
    },
}

pub struct Als {
    sensor: SensorType,
    thresholds: HashMap<u64, String>,
}

impl Als {
    pub fn new(base_path: &str, thresholds: HashMap<u64, String>) -> Result<Self, Box<dyn Error>> {
        Path::new(base_path)
            .read_dir()
            .ok()
            .and_then(|dir| {
                dir.filter_map(|e| e.ok())
                    .find(|e| {
                        ["als", "acpi-als", "apds9960"].contains(
                            &fs::read_to_string(e.path().join("name"))
                                .unwrap_or_default()
                                .trim(),
                        )
                    })
                    .and_then(|e| {
                        // TODO should probably start from the `parse_illuminance_input` in the next major version
                        parse_illuminance_raw(e.path())
                            .or_else(|_| parse_illuminance_input(e.path()))
                            .or_else(|_| parse_intensity_raw(e.path()))
                            .or_else(|_| parse_intensity_rgb(e.path()))
                            .ok()
                    })
            })
            .map(|sensor| Self { sensor, thresholds })
            .ok_or_else(|| "No iio device found".into())
    }

    fn get_raw(&self) -> Result<u64, Box<dyn Error>> {
        Ok(match self.sensor {
            Illuminance {
                ref value,
                scale,
                offset,
            } => (read(&mut value.lock().unwrap())? + offset) * scale,

            Intensity {
                ref r,
                ref g,
                ref b,
            } => {
                -0.32466 * read(&mut r.lock().unwrap())?
                    + 1.57837 * read(&mut g.lock().unwrap())?
                    + -0.73191 * read(&mut b.lock().unwrap())?
            }
        } as u64)
    }
}

impl super::Als for Als {
    fn get(&self) -> Result<String, Box<dyn Error>> {
        let raw = self.get_raw()?;
        let profile = super::find_profile(raw, &self.thresholds);

        log::trace!("ALS (iio): {} ({})", profile, raw);
        Ok(profile)
    }
}

fn parse_illuminance_raw(path: PathBuf) -> Result<SensorType, Box<dyn Error>> {
    Ok(Illuminance {
        value: Mutex::new(
            open_file(&path, "in_illuminance_raw")
                .or_else(|_| open_file(&path, "in_illuminance0_raw"))?,
        ),
        scale: open_file(&path, "in_illuminance_scale")
            .or_else(|_| open_file(&path, "in_illuminance0_scale"))
            .and_then(|mut f| read(&mut f))
            .unwrap_or(1_f64),
        offset: open_file(&path, "in_illuminance_offset")
            .or_else(|_| open_file(&path, "in_illuminance0_offset"))
            .and_then(|mut f| read(&mut f))
            .unwrap_or(0_f64),
    })
}

fn parse_intensity_raw(path: PathBuf) -> Result<SensorType, Box<dyn Error>> {
    Ok(Illuminance {
        value: Mutex::new(open_file(&path, "in_intensity_both_raw")?),
        scale: open_file(&path, "in_intensity_scale")
            .and_then(|mut f| read(&mut f))
            .unwrap_or(1_f64),
        offset: open_file(&path, "in_intensity_offset")
            .and_then(|mut f| read(&mut f))
            .unwrap_or(0_f64),
    })
}

fn parse_illuminance_input(path: PathBuf) -> Result<SensorType, Box<dyn Error>> {
    Ok(Illuminance {
        value: Mutex::new(
            open_file(&path, "in_illuminance_input")
                .or_else(|_| open_file(&path, "in_illuminance0_input"))?,
        ),
        scale: 1_f64,
        offset: 0_f64,
    })
}

fn parse_intensity_rgb(path: PathBuf) -> Result<SensorType, Box<dyn Error>> {
    Ok(Intensity {
        r: Mutex::new(open_file(&path, "in_intensity_red_raw")?),
        g: Mutex::new(open_file(&path, "in_intensity_green_raw")?),
        b: Mutex::new(open_file(&path, "in_intensity_blue_raw")?),
    })
}

fn open_file(path: &Path, name: &str) -> Result<File, Box<dyn Error>> {
    File::open(path.join(name)).map_err(Box::<dyn Error>::from)
}
