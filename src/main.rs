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

    env_logger::init();

    let config = match config::load() {
        Ok(config) => config,
        Err(err) => panic!("Unable to load config: {}", err),
    };

    log::debug!("Using config: {:?}", config);

    if config.output.is_empty() {
        log::error!("No devices configured, aborting");
        std::process::exit(1);
    }

    let als_txs = config
        .output
        .iter()
        .map(|output| {
            let output = output.clone();

            let (als_tx, als_rx) = mpsc::channel();
            let (user_tx, user_rx) = mpsc::channel();
            let (prediction_tx, prediction_rx) = mpsc::channel();

            let (config_output_name, config_output_capturer) = match output.clone() {
                config::Output::Backlight(cfg) => (cfg.name, cfg.capturer),
                config::Output::DdcUtil(cfg) => (cfg.name, cfg.capturer),
            };

            let output_name = config_output_name.clone();
            let thread_name = format!("backlight-{}", output_name);

            std::thread::Builder::new()
                .name(thread_name.clone())
                .spawn(move || {
                    let brightness = match output {
                        config::Output::Backlight(cfg) => {
                            brightness::Backlight::new(&cfg.path, cfg.min_brightness)
                                .map(|b| Box::new(b) as Box<dyn brightness::Brightness>)
                        }
                        config::Output::DdcUtil(cfg) => {
                            brightness::DdcUtil::new(&cfg.name, cfg.min_brightness)
                                .map(|b| Box::new(b) as Box<dyn brightness::Brightness>)
                        }
                    };
                    match brightness {
                        Ok(b) => {
                            brightness::Controller::new(b, user_tx, prediction_rx).run();
                        }
                        Err(err) => log::warn!(
                            "Skipping '{}' as it might be disconnected: {}",
                            output_name,
                            err
                        ),
                    };
                })
                .unwrap_or_else(|_| panic!("Unable to start thread: {}", thread_name));

            let output_name = config_output_name;
            let thread_name = format!("predictor-{}", output_name);

            std::thread::Builder::new()
                .name(thread_name.clone())
                .spawn(move || {
                    let frame_processor: Box<dyn frame::processor::Processor> = Box::new(
                        frame::processor::vulkan::Processor::new()
                            .expect("Unable to initialize Vulkan"),
                    );
                    let frame_capturer: Box<dyn frame::capturer::Capturer> =
                        match config_output_capturer {
                            config::Capturer::Wlroots => {
                                Box::new(frame::capturer::wlroots::Capturer::new(frame_processor))
                            }
                            config::Capturer::None => {
                                Box::new(frame::capturer::none::Capturer::default())
                            }
                        };

                    let controller = predictor::Controller::new(
                        prediction_tx,
                        user_rx,
                        als_rx,
                        true,
                        &output_name,
                    );
                    frame_capturer.run(&output_name, controller)
                })
                .unwrap_or_else(|_| panic!("Unable to start thread: {}", thread_name));

            als_tx
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
                config::Als::None => Box::new(als::none::Als::default()),
            };

            als::controller::Controller::new(als, als_txs).run();
        })
        .expect("Unable to start thread: als");

    println!("Continue adjusting brightness and wluma will learn your preference over time.");
    std::thread::park();
}
