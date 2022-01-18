use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum Capturer {
    Wlroots,
    None,
}

#[derive(Debug)]
pub enum Als {
    Iio {
        path: String,
        thresholds: HashMap<u64, String>,
    },
    Time {
        thresholds: HashMap<u64, String>,
    },
    Webcam {
        video: usize,
        thresholds: HashMap<u64, String>,
    },
    None,
}

#[derive(Debug, Clone)]
pub struct BacklightOutput {
    pub name: String,
    pub path: String,
    pub capturer: Capturer,
}

#[derive(Debug, Clone)]
pub struct DdcUtilOutput {
    pub name: String,
    pub capturer: Capturer,
}

#[derive(Debug, Clone)]
pub enum Output {
    Backlight(BacklightOutput),
    DdcUtil(DdcUtilOutput),
}

#[derive(Debug, Default)]
pub struct Keyboards {
    pub backlight: HashMap<String, Keyboard>,
}

#[derive(Debug)]
pub struct Keyboard {
    pub path: String,
}

#[derive(Debug)]
pub struct Config {
    pub als: Als,
    pub keyboard: Option<Keyboards>,
    pub output: Vec<Output>,
}
