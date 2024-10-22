use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum Capturer {
    WlrExportDmabufUnstableV1,
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
    pub min_brightness: u64,
}

#[derive(Debug, Clone)]
pub struct DdcUtilOutput {
    pub name: String,
    pub capturer: Capturer,
    pub min_brightness: u64,
}

#[derive(Debug, Clone)]
pub enum Output {
    Backlight(BacklightOutput),
    DdcUtil(DdcUtilOutput),
}

#[derive(Debug)]
pub struct Config {
    pub als: Als,
    pub output: Vec<Output>,
}
