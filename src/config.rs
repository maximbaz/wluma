use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Capturer {
    Wlroots,
    None,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum Processor {
    Vulkan,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Frame {
    pub capturer: Capturer,
    pub processor: Processor,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Als {
    Iio {
        path: String,
        #[serde(rename = "thresholds")]
        thresholds_string_key: HashMap<String, String>,
        #[serde(skip)]
        thresholds: HashMap<u64, String>,
    },
    Time {
        #[serde(rename = "thresholds")]
        thresholds_string_key: HashMap<String, String>,
        #[serde(skip)]
        thresholds: HashMap<u64, String>,
    },
    Webcam {
        video: usize,
        #[serde(rename = "thresholds")]
        thresholds_string_key: HashMap<String, String>,
        #[serde(skip)]
        thresholds: HashMap<u64, String>,
    },
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
    pub name: String,
    pub path: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct DdcUtilOutput {
    pub name: String,
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
                .map(Output::Backlight)
                .chain(cfg.output_by_type.ddcutil.into_iter().map(Output::DdcUtil))
                .collect();
            cfg.output_by_type = OutputByType::default();

            let parse_als_thresholds = |t: HashMap<String, String>| -> HashMap<u64, String> {
                t.into_iter()
                    .map(|(k, v)| (k.parse::<u64>().unwrap(), v))
                    .collect()
            };

            cfg.als = match cfg.als {
                Als::Iio {
                    path,
                    thresholds_string_key,
                    ..
                } => Als::Iio {
                    path,
                    thresholds: parse_als_thresholds(thresholds_string_key),
                    thresholds_string_key: HashMap::default(),
                },
                Als::Webcam {
                    video,
                    thresholds_string_key,
                    ..
                } => Als::Webcam {
                    video,
                    thresholds: parse_als_thresholds(thresholds_string_key),
                    thresholds_string_key: HashMap::default(),
                },
                Als::Time {
                    thresholds_string_key,
                    ..
                } => Als::Time {
                    thresholds: parse_als_thresholds(thresholds_string_key),
                    thresholds_string_key: HashMap::default(),
                },
                Als::None => Als::None,
            };

            cfg
        })
    }
}
