use itertools::Itertools;
use std::sync::mpsc;

mod als;
mod brightness;
mod config;
mod device_file;
mod frame;
mod predictor;

fn main() {
    env_logger::init();

    let config = match config::Config::load() {
        Ok(config) => config,
        Err(err) => panic!("Unable to load config: {}", err),
    };

    log::debug!("Using config: {:?}", config);

    let config_outputs = config.output;

    let threads = config_outputs
        .into_iter()
        .flat_map(move |output| {
            let config = match config::Config::load() {
                Ok(config) => config,
                Err(err) => panic!("Unable to load config: {}", err),
            };

            let (user_tx, user_rx) = mpsc::channel();
            let (prediction_tx, prediction_rx) = mpsc::channel();

            let capturer_config = output.clone();

            vec![
                std::thread::spawn(move || {
                    let brightness: Box<dyn brightness::Brightness> = match output {
                        config::Output::Backlight(cfg) => Box::new(
                            brightness::Backlight::new(&cfg.path)
                                .expect("Unable to initialize output backlight"),
                        ),
                        config::Output::DdcUtil(cfg) => Box::new(
                            brightness::DdcUtil::new(&cfg.serial_number)
                                .expect("Unable to initialize output ddcutil"),
                        ),
                    };

                    let mut brightness_controller =
                        brightness::Controller::new(brightness, user_tx, prediction_rx);

                    brightness_controller
                        .run()
                        .expect("Error running brightness controller thread");
                }),
                std::thread::spawn(move || {
                    let als: Box<dyn als::Als> = match config.als {
                        config::Als::Iio { path, thresholds } => Box::new(
                            als::iio::Als::new(&path, thresholds)
                                .expect("Unable to initialize ALS IIO sensor"),
                        ),
                        config::Als::Time { ref hour_to_lux } => {
                            Box::new(als::time::Als::new(hour_to_lux))
                        }
                        config::Als::Webcam { video, thresholds } => Box::new({
                            let (webcam_tx, webcam_rx) = mpsc::channel();
                            std::thread::spawn(move || {
                                als::webcam::Webcam::new(webcam_tx, video).run();
                            });

                            als::webcam::Als::new(webcam_rx, thresholds)
                        }),
                        config::Als::None => Box::new(als::none::Als::default()),
                    };

                    let frame_processor: Box<dyn frame::processor::Processor> =
                        match config.frame.processor {
                            config::Processor::Vulkan => Box::new(
                                frame::processor::vulkan::Processor::new()
                                    .expect("Unable to initialize Vulkan"),
                            ),
                        };

                    let frame_capturer: Box<dyn frame::capturer::Capturer> =
                        match config.frame.capturer {
                            config::Capturer::Wlroots => {
                                Box::new(frame::capturer::wlroots::Capturer::new(frame_processor))
                            }
                            config::Capturer::None => {
                                Box::new(frame::capturer::none::Capturer::default())
                            }
                        };

                    let model = match capturer_config {
                        config::Output::Backlight(cfg) => cfg.model,
                        config::Output::DdcUtil(cfg) => cfg.model,
                    };

                    let controller =
                        predictor::Controller::new(prediction_tx, user_rx, als, true, &model);
                    frame_capturer.run(&model, controller)
                }),
            ]
        })
        .collect_vec();

    println!("Continue adjusting brightness and wluma will learn your preference over time.");

    threads
        .into_iter()
        .for_each(|t| t.join().expect("Error running thread"));
}
