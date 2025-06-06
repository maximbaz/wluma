use crate::device_file::read;
use crate::ErrorBox;
use smol::fs::{self, File};
use smol::lock::Mutex;
use smol::stream::StreamExt;
use std::collections::HashMap;
use std::error::Error;
use std::ops::DerefMut;
use std::path::{Path, PathBuf};
use SensorType::*;

#[allow(clippy::large_enum_variant)]
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
    pub async fn new(base_path: &str, thresholds: HashMap<u64, String>) -> Result<Self, ErrorBox> {
        let mut dir_stream = smol::fs::read_dir(base_path)
            .await
            .map_err(|e| ErrorBox::from(format!("Can't enumerate iio devices: {e}")))?;

        let e = 'find_e: {
            while let Some(dir) = dir_stream.next().await {
                let Ok(dir) = dir else { continue };

                let name = fs::read_to_string(dir.path().join("name"))
                    .await
                    .unwrap_or_default();
                let name = name.trim();

                if ["als", "acpi-als", "apds9960"].contains(&name) {
                    break 'find_e dir;
                }
            }
            return Err("No iio device found".into());
        };

        let sensor = 'sensor: {
            // TODO should probably start from the `parse_illuminance_input` in the next major version
            if let Ok(s) = parse_illuminance_raw(e.path()).await {
                break 'sensor s;
            }
            if let Ok(s) = parse_illuminance_input(e.path()).await {
                break 'sensor s;
            }
            if let Ok(s) = parse_intensity_raw(e.path()).await {
                break 'sensor s;
            }
            if let Ok(s) = parse_intensity_rgb(e.path()).await {
                break 'sensor s;
            }

            return Err(format!("failed to read sensor '{}'", e.path().display()).into());
        };

        Ok(Self { sensor, thresholds })
    }

    pub async fn get(&self) -> Result<String, ErrorBox> {
        let raw = self.get_raw().await?;
        let profile = super::find_profile(raw, &self.thresholds);

        log::trace!("ALS (iio): {} ({})", profile, raw);
        Ok(profile)
    }

    async fn get_raw(&self) -> Result<u64, ErrorBox> {
        Ok(match self.sensor {
            Illuminance {
                ref value,
                scale,
                offset,
            } => (read(value.lock().await.deref_mut()).await? + offset) * scale,

            Intensity {
                ref r,
                ref g,
                ref b,
            } => {
                -0.32466 * read(r.lock().await.deref_mut()).await?
                    + 1.57837 * read(g.lock().await.deref_mut()).await?
                    + -0.73191 * read(b.lock().await.deref_mut()).await?
            }
        } as u64)
    }
}

async fn parse_illuminance_raw(path: PathBuf) -> Result<SensorType, ErrorBox> {
    Ok(Illuminance {
        value: Mutex::new(
            if let Ok(f) = open_file(&path, "in_illuminance_raw").await {
                f
            } else {
                open_file(&path, "in_illuminance0_raw").await?
            },
        ),
        scale: {
            let mut f = if let Ok(f) = open_file(&path, "in_illuminance_scale").await {
                f
            } else {
                open_file(&path, "in_illuminance0_scale").await?
            };

            read(&mut f).await.unwrap_or(1_f64)
        },
        offset: {
            let mut f = if let Ok(f) = open_file(&path, "in_illuminance_offset").await {
                f
            } else {
                open_file(&path, "in_illuminance0_offset").await?
            };

            read(&mut f).await.unwrap_or(0_f64)
        },
    })
}

async fn parse_intensity_raw(path: PathBuf) -> Result<SensorType, ErrorBox> {
    async fn try_open_and_read(path: &Path, name: &str) -> Result<f64, ErrorBox> {
        let mut f = open_file(path, name).await?;
        read(&mut f).await
    }

    Ok(Illuminance {
        value: Mutex::new(open_file(&path, "in_intensity_both_raw").await?),
        scale: try_open_and_read(&path, "in_intensity_scale")
            .await
            .unwrap_or(1_f64),
        offset: try_open_and_read(&path, "in_intensity_offset")
            .await
            .unwrap_or(0_f64),
    })
}

async fn parse_illuminance_input(path: PathBuf) -> Result<SensorType, ErrorBox> {
    Ok(Illuminance {
        value: Mutex::new(
            if let Ok(f) = open_file(&path, "in_illuminance_input").await {
                f
            } else {
                open_file(&path, "in_illuminance0_input").await?
            },
        ),
        scale: 1_f64,
        offset: 0_f64,
    })
}

async fn parse_intensity_rgb(path: PathBuf) -> Result<SensorType, ErrorBox> {
    Ok(Intensity {
        r: Mutex::new(open_file(&path, "in_intensity_red_raw").await?),
        g: Mutex::new(open_file(&path, "in_intensity_green_raw").await?),
        b: Mutex::new(open_file(&path, "in_intensity_blue_raw").await?),
    })
}

async fn open_file(path: &Path, name: &str) -> Result<File, ErrorBox> {
    File::open(path.join(name))
        .await
        .map_err(Box::<dyn Error + Send + Sync>::from)
}
