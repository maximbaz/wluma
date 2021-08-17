use brightness::Backlight;
use config::Config;
use controller::Controller;
use frame::wlroots::Wlroots;

mod als;
mod brightness;
mod config;
mod controller;
mod device_file;
mod frame;
mod vulkan;

fn main() {
    let config = match Config::load() {
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

    let frame_capturer: Box<dyn frame::Capturer> = match config.screen_contents.capturer {
        config::Capturer::Wlroots => Box::new(Wlroots::default()),
        config::Capturer::None => Box::new(frame::none::Capturer::default()),
    };

    let brightness = match config.output.iter().next().unwrap().1 {
        config::Output::Backlight(cfg) => {
            Box::new(Backlight::new(&cfg.path).expect("Unable to initialize output backlight"))
        }
        _ => unimplemented!("Only backlight-controlled outputs are supported"),
    };

    let controller = Controller::new(brightness, als, true);

    println!("Continue adjusting brightness and wluma will learn your preference over time.");
    frame_capturer.run(controller);
}
