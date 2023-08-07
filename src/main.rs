use config::Output;
use itertools::Itertools;
use std::error::Error;
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
            init_output(output)
                .map_err(|err| {
                    log::warn!(
                        "Skipping '{}' as it might be disconnected: {}",
                        output.name(),
                        err
                    )
                })
                .ok()
        })
        .collect_vec();

    spawn("als".to_string(), move || {
        let als: Box<dyn als::Als> = match config.als {
            config::Als::Iio { path, thresholds } => Box::new(
                als::iio::Als::new(&path, thresholds).expect("Unable to initialize ALS IIO sensor"),
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
    });

    log::info!("Continue adjusting brightness and wluma will learn your preference over time.");
    std::thread::park();
}

fn spawn<F, T>(thread_name: String, handler: F)
where
    F: FnOnce() -> T,
    F: Send + 'static,
    T: Send + 'static,
{
    std::thread::Builder::new()
        .name(thread_name.clone())
        .spawn(handler)
        .unwrap_or_else(|_| panic!("Unable to start thread: {}", thread_name));
}

fn init_output(output: &Output) -> Result<mpsc::Sender<std::string::String>, Box<dyn Error>> {
    let (als_tx, als_rx) = mpsc::channel();
    let (user_tx, user_rx) = mpsc::channel();
    let (prediction_tx, prediction_rx) = mpsc::channel();

    let brightness = match &output {
        config::Output::Backlight(cfg) => {
            brightness::Backlight::new(&cfg.path, cfg.min_brightness).map(|b| Box::new(b) as Box<_>)
        }
        config::Output::DdcUtil(cfg) => {
            brightness::DdcUtil::new(&cfg.name, cfg.min_brightness).map(|b| Box::new(b) as Box<_>)
        }
    };

    match brightness {
        Ok(brightness) => {
            spawn(format!("backlight-{}", output.name()), move || {
                brightness::Controller::new(brightness, user_tx, prediction_rx).run()
            });

            let output = output.clone();
            spawn(format!("predictor-{}", output.name()), move || {
                let predictor_controller =
                    predictor::Controller::new(prediction_tx, user_rx, als_rx, true, output.name());

                let frame_capturer: Result<Box<dyn frame::capturer::Capturer>, _> = match output
                    .capturer()
                {
                    config::Capturer::Wlroots => {
                        frame::capturer::wlroots::Capturer::new(output.name(), predictor_controller)
                            .map(|b| Box::new(b) as Box<_>)
                    }
                    config::Capturer::None => {
                        frame::capturer::none::Capturer::new(predictor_controller)
                            .map(|b| Box::new(b) as Box<_>)
                    }
                };

                match frame_capturer {
                    Ok(mut frame_capturer) => frame_capturer.run(),
                    Err(err) => log::warn!(
                        "Skipping '{}' as unable to initialize frame capturer, it might be disconnected: {}",
                        output.name(),
                        err
                    ),
                }
            });
        }
        Err(err) => log::warn!(
            "Skipping '{}' as unable to initialize brightness controller, it might be disconnected: {}",
            output.name(),
            err
        ),
    };

    Ok(als_tx)
}
