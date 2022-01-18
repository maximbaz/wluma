use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Capturer {
    Wlroots,
    None,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Als {
    Iio {
        path: String,
        thresholds: HashMap<String, String>,
    },
    Time {
        thresholds: HashMap<String, String>,
    },
    Webcam {
        video: usize,
        thresholds: HashMap<String, String>,
    },
    None,
}

#[derive(Deserialize, Debug, Default)]
#[serde(default)]
pub struct OutputByType {
    pub backlight: Vec<BacklightOutput>,
    pub ddcutil: Vec<DdcUtilOutput>,
}

#[derive(Deserialize, Debug)]
pub struct BacklightOutput {
    pub name: String,
    pub path: String,
    pub capturer: Capturer,
}

#[derive(Deserialize, Debug)]
pub struct DdcUtilOutput {
    pub name: String,
    pub capturer: Capturer,
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
    pub als: Als,
    pub output: OutputByType,
    pub keyboard: Option<Keyboards>,
}
