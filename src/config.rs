use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Capturer {
    Wlroots,
    None,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Processor {
    Vulkan,
}

#[derive(Deserialize, Debug)]
pub struct Frame {
    pub capturer: Capturer,
    pub processor: Processor,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Als {
    Iio { path: String, thresholds: Vec<u64> },
    Time { hour_to_lux: HashMap<String, u64> },
    Webcam { video: usize, thresholds: Vec<u64> },
    None,
}

#[derive(Deserialize, Debug, Default)]
#[serde(default)]
pub struct OutputByType {
    pub backlight: Vec<BacklightOutput>,
    pub ddcutil: Vec<DdcUtilOutput>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct BacklightOutput {
    pub path: String,
    pub model: String,
    #[serde(default)]
    pub use_contents: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DdcUtilOutput {
    pub model: String,
    pub serial_number: String,
    #[serde(default)]
    pub use_contents: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub enum Output {
    Backlight(BacklightOutput),
    DdcUtil(DdcUtilOutput),
}

#[derive(Deserialize, Debug, Default)]
#[serde(default)]
pub struct Keyboards {
    pub backlight: HashMap<String, Keyboard>,
}

#[derive(Deserialize, Debug)]
pub struct Keyboard {
    pub path: String,
}

#[derive(Deserialize, Debug)]
pub struct Config {
    pub frame: Frame,
    pub als: Als,
    #[serde(rename = "output")]
    output_by_type: OutputByType,
    pub keyboard: Option<Keyboards>,
    #[serde(skip)]
    pub output: Vec<Output>,
}

impl Config {
    pub fn load() -> Result<Self, toml::de::Error> {
        let config = dirs::config_dir()
            .and_then(|config_dir| fs::read_to_string(&config_dir.join("wluma/config.toml")).ok())
            .unwrap_or_else(|| include_str!("../config.toml").to_string());

        toml::from_str(&config).map(|mut cfg: Self| {
            cfg.output = cfg
                .output_by_type
                .backlight
                .into_iter()
                .map(|output| Output::Backlight(output))
                .chain(
                    cfg.output_by_type
                        .ddcutil
                        .into_iter()
                        .map(|output| Output::DdcUtil(output)),
                )
                .collect();
            cfg.output_by_type = OutputByType::default();
            cfg
        })
    }
}
