use std::collections::HashMap;
use std::fs;

pub mod app;
pub mod file;

pub fn load() -> Result<app::Config, toml::de::Error> {
    let file_config = dirs::config_dir()
        .and_then(|config_dir| fs::read_to_string(&config_dir.join("wluma/config.toml")).ok())
        .unwrap_or_else(|| include_str!("../../config.toml").to_string());

    toml::from_str(&file_config).map(|cfg: file::Config| {
        let parse_als_thresholds = |t: HashMap<String, String>| -> HashMap<u64, String> {
            t.into_iter()
                .map(|(k, v)| (k.parse::<u64>().unwrap(), v))
                .collect()
        };

        app::Config {
            keyboard: None,
            output: cfg
                .output
                .backlight
                .into_iter()
                .map(|x| app::Output::Backlight {
                    0: app::BacklightOutput {
                        name: x.name,
                        path: x.path,
                        capturer: match x.capturer {
                            file::Capturer::None => app::Capturer::None,
                            file::Capturer::Wlroots => app::Capturer::Wlroots,
                        },
                    },
                })
                .chain(
                    cfg.output
                        .ddcutil
                        .into_iter()
                        .map(|x| app::Output::DdcUtil {
                            0: app::DdcUtilOutput {
                                name: x.name,
                                capturer: match x.capturer {
                                    file::Capturer::None => app::Capturer::None,
                                    file::Capturer::Wlroots => app::Capturer::Wlroots,
                                },
                            },
                        }),
                )
                .collect(),

            als: match cfg.als {
                file::Als::Iio { path, thresholds } => app::Als::Iio {
                    path,
                    thresholds: parse_als_thresholds(thresholds),
                },
                file::Als::Webcam { video, thresholds } => app::Als::Webcam {
                    video,
                    thresholds: parse_als_thresholds(thresholds),
                },
                file::Als::Time { thresholds } => app::Als::Time {
                    thresholds: parse_als_thresholds(thresholds),
                },
                file::Als::None => app::Als::None,
            },
        }
    })
}
