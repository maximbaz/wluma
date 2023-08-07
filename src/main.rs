use itertools::Itertools;
use std::sync::mpsc;

mod als;
mod brightness;
mod config;
mod device_file;
mod frame;
mod predictor;

fn main() {
    let panic_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        panic_hook(panic_info);
        std::process::exit(1);
    }));

    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .init();

    let config = match config::load() {
        Ok(config) => config,
        Err(err) => panic!("Unable to load config: {}", err),
    };

    log::debug!("Using {:#?}", config);

    let als_txs = config
        .output
        .iter()
        .filter_map(|output| {
            let (als_tx, als_rx) = mpsc::channel();
            let (user_tx, user_rx) = mpsc::channel();
            let (prediction_tx, prediction_rx) = mpsc::channel();

            let brightness = match output {
                config::Output::Backlight(cfg) => {
                    brightness::Backlight::new(&cfg.path, cfg.min_brightness)
                        .map(|b| Box::new(b) as Box<dyn brightness::Brightness + Send>)
                }
                config::Output::DdcUtil(cfg) => {
                    brightness::DdcUtil::new(&cfg.name, cfg.min_brightness)
                        .map(|b| Box::new(b) as Box<dyn brightness::Brightness + Send>)
                }
            };

            match brightness {
                Ok(b) => {
                    let thread_name = format!("backlight-{}", output.name());
                    std::thread::Builder::new()
                        .name(thread_name.clone())
                        .spawn(move || {
                            brightness::Controller::new(b, user_tx, prediction_rx).run();
                        })
                        .unwrap_or_else(|_| panic!("Unable to start thread: {}", thread_name));

                    let output = output.clone();
                    let thread_name = format!("predictor-{}", output.name());
                    std::thread::Builder::new()
                        .name(thread_name.clone())
                        .spawn(move || {
                            let controller = predictor::Controller::new(
                                prediction_tx,
                                user_rx,
                                als_rx,
                                true,
                                output.name(),
                            );

                            let mut frame_capturer: Box<dyn frame::capturer::Capturer> =
                                match output.capturer() {
                                    config::Capturer::Wlroots => Box::new(
                                        frame::capturer::wlroots::Capturer::new(
                                            output.name(),
                                            controller,
                                        )
                                        .unwrap(),
                                    ),
                                    config::Capturer::None => Box::new(
                                        frame::capturer::none::Capturer::new(controller).unwrap(),
                                    ),
                                };

                            frame_capturer.run()
                        })
                        .unwrap_or_else(|_| panic!("Unable to start thread: {}", thread_name));

                    Some(als_tx)
                }
                Err(err) => {
                    log::warn!(
                        "Skipping '{}' as it might be disconnected: {}",
                        output.name(),
                        err
                    );

                    None
                }
            }
        })
        .collect_vec();

    std::thread::Builder::new()
        .name("als".to_string())
        .spawn(move || {
            let als: Box<dyn als::Als> = match config.als {
                config::Als::Iio { path, thresholds } => Box::new(
                    als::iio::Als::new(&path, thresholds)
                        .expect("Unable to initialize ALS IIO sensor"),
                ),
                config::Als::Time { thresholds } => Box::new(als::time::Als::new(thresholds)),
                config::Als::Webcam { video, thresholds } => Box::new({
                    let (webcam_tx, webcam_rx) = mpsc::channel();
                    std::thread::Builder::new()
                        .name("als-webcam".to_string())
                        .spawn(move || {
                            als::webcam::Webcam::new(webcam_tx, video).run();
                        })
                        .expect("Unable to start thread: als-webcam");
                    als::webcam::Als::new(webcam_rx, thresholds)
                }),
                config::Als::None => Box::<als::none::Als>::default(),
            };

            als::controller::Controller::new(als, als_txs).run();
        })
        .expect("Unable to start thread: als");

    log::info!("Continue adjusting brightness and wluma will learn your preference over time.");
    std::thread::park();
}
