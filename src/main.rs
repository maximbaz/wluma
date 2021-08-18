mod als;
mod brightness;
mod config;
mod controller;
mod device_file;
mod frame;

fn main() {
    let config = match config::Config::load() {
        Ok(config) => config,
        Err(err) => panic!("Unable to load config: {}", err),
    };
    println!("Using config: {:?}", config);

    let als: Box<dyn als::Als> = match config.als {
        config::Als::Iio { ref path } => {
            Box::new(als::iio::Als::new(path).expect("Unable to initialize ALS IIO sensor"))
        }
        config::Als::Time { ref hour_to_lux } => Box::new(als::time::Als::new(hour_to_lux)),
        config::Als::None => Box::new(als::none::Als::default()),
    };

    let frame_processor: Box<dyn frame::processor::Processor> = match config.frame.processor {
        config::Processor::Vulkan => Box::new(
            frame::processor::vulkan::Processor::new().expect("Unable to initialize Vulkan"),
        ),
    };

    let frame_capturer: Box<dyn frame::capturer::Capturer> = match config.frame.capturer {
        config::Capturer::Wlroots => {
            Box::new(frame::capturer::wlroots::Capturer::new(frame_processor))
        }
        config::Capturer::None => Box::new(frame::capturer::none::Capturer::default()),
    };

    let brightness = match config.output.iter().next().unwrap().1 {
        config::Output::Backlight(cfg) => Box::new(
            brightness::Backlight::new(&cfg.path).expect("Unable to initialize output backlight"),
        ),
        _ => unimplemented!("Only backlight-controlled outputs are supported"),
    };

    let controller = controller::Controller::new(brightness, als, true);

    println!("Continue adjusting brightness and wluma will learn your preference over time.");
    frame_capturer.run(controller);
}
