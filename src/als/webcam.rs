use crate::als::smoothen;
use crate::frame::compute_perceived_lightness_percent;
use crate::predictor::kalman::Kalman;
use std::cell::RefCell;
use std::error::Error;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::Duration;
use v4l::buffer::Type;
use v4l::io::mmap::Stream;
use v4l::io::traits::CaptureStream;
use v4l::video::Capture;
use v4l::{Device, FourCC};

const DEFAULT_LUX: u64 = 100;
const WAITING_SLEEP_MS: u64 = 2000;

pub struct Webcam {
    kalman: Kalman,
    webcam_tx: Sender<u64>,
    video: usize,
}

impl Webcam {
    pub fn new(webcam_tx: Sender<u64>, video: usize) -> Self {
        Self {
            kalman: Kalman::new(1.0, 20.0, 10.0),
            webcam_tx,
            video,
        }
    }

    pub fn run(&mut self) -> Result<(), Box<dyn Error>> {
        loop {
            self.step()?;
        }
    }

    fn step(&mut self) -> Result<(), Box<dyn Error>> {
        if let Ok((rgbs, pixels)) = self.frame() {
            let lux_raw = compute_perceived_lightness_percent(&rgbs, false, pixels) as u64;
            let lux = self.kalman.process(lux_raw);

            self.webcam_tx.send(lux)?;
        };

        thread::sleep(Duration::from_millis(WAITING_SLEEP_MS));
        Ok(())
    }

    fn frame(&mut self) -> Result<(Vec<u8>, usize), Box<dyn Error>> {
        let dev = Device::new(self.video)?;

        let mut fmt = dev.format()?;
        fmt.fourcc = FourCC::new(b"RGB3");
        dev.set_format(&fmt)?;

        let mut stream = Stream::new(&dev, Type::VideoCapture)?;
        let (rgbs, _) = stream.next()?;

        Ok((rgbs.to_vec(), fmt.height as usize * fmt.width as usize))
    }
}

pub struct Als {
    webcam_rx: Receiver<u64>,
    thresholds: Vec<u64>,
    lux: RefCell<u64>,
}

impl Als {
    pub fn new(webcam_rx: Receiver<u64>, thresholds: Vec<u64>) -> Self {
        Self {
            webcam_rx,
            thresholds,
            lux: RefCell::new(DEFAULT_LUX),
        }
    }
}

impl super::Als for Als {
    fn get_raw(&self) -> Result<u64, Box<dyn Error>> {
        let value = *self.lux.borrow();
        *self.lux.borrow_mut() = self.webcam_rx.try_iter().last().unwrap_or(value);
        Ok(value)
    }

    fn smoothen(&self, raw: u64) -> u64 {
        smoothen(raw, &self.thresholds)
    }
}
