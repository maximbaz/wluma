use crate::als::smoothen;
use crate::device_file::read;
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
    thresholds: Vec<u64>,
}

impl Als {
    pub fn new(base_path: &str, thresholds: Vec<u64>) -> Result<Self, Box<dyn Error>> {
        Path::new(base_path)
            .read_dir()
            .ok()
            .and_then(|dir| {
                dir.filter_map(|e| e.ok())
                    .find(|e| {
                        fs::read_to_string(e.path().join("name"))
                            .unwrap_or_default()
                            .trim()
                            == "als"
                    })
                    .and_then(|e| {
                        parse_illuminance(e.path())
                            .or_else(|_| parse_intensity(e.path()))
                            .ok()
                    })
            })
            .map(|sensor| Self { sensor, thresholds })
            .ok_or_else(|| "No iio device found".into())
    }
}

impl super::Als for Als {
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

    fn smoothen(&self, raw: u64) -> u64 {
        smoothen(raw, &self.thresholds)
    }
}

fn parse_illuminance(path: PathBuf) -> Result<SensorType, Box<dyn Error>> {
    Ok(Illuminance {
        value: Mutex::new(File::open(path.join("in_illuminance_raw"))?),
        scale: read(&mut File::open(path.join("in_illuminance_scale"))?)?,
        offset: read(&mut File::open(path.join("in_illuminance_offset"))?)?,
    })
}

fn parse_intensity(path: PathBuf) -> Result<SensorType, Box<dyn Error>> {
    Ok(Intensity {
        r: Mutex::new(File::open(path.join("in_intensity_red_raw"))?),
        g: Mutex::new(File::open(path.join("in_intensity_green_raw"))?),
        b: Mutex::new(File::open(path.join("in_intensity_blue_raw"))?),
    })
}
