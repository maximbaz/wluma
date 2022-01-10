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

    pub fn run(&mut self) {
        loop {
            self.step();
        }
    }

    fn step(&mut self) {
        if let Ok((rgbs, pixels)) = self.frame() {
            let lux_raw = compute_perceived_lightness_percent(&rgbs, false, pixels) as u64;
            let lux = self.kalman.process(lux_raw);

            self.webcam_tx
                .send(lux)
                .expect("Unable to send new webcam lux value, channel is dead");
        };

        thread::sleep(Duration::from_millis(WAITING_SLEEP_MS));
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
        let new_value = self
            .webcam_rx
            .try_iter()
            .last()
            .unwrap_or(*self.lux.borrow());
        *self.lux.borrow_mut() = new_value;
        Ok(new_value)
    }

    fn smoothen(&self, raw: u64) -> u64 {
        smoothen(raw, &self.thresholds)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Als as AlsTrait;
    use super::*;

    use std::sync::mpsc;

    fn setup() -> (Als, Sender<u64>) {
        let (webcam_tx, webcam_rx) = mpsc::channel();
        let als = Als::new(webcam_rx, vec![]);
        (als, webcam_tx)
    }

    #[test]
    fn test_get_raw_if_initial_value_is_the_good() -> Result<(), Box<dyn Error>> {
        let (als, _) = setup();
        let value = als.get_raw()?;
        // we dont send data to the channel
        // so this should return the default lux value
        assert_eq!(DEFAULT_LUX, value);
        Ok(())
    }

    #[test]
    fn test_get_raw_if_received_data_the_are_those_send_before() -> Result<(), Box<dyn Error>> {
        let (als, webcam_tx) = setup();
        // until we send data
        webcam_tx.send(42)?;
        // and this well return the same data
        let value = als.get_raw()?;
        assert_eq!(42, value);
        Ok(())
    }

    #[test]
    fn test_get_raw_if_data_is_the_same_if_we_receive_twice() -> Result<(), Box<dyn Error>> {
        let (als, webcam_tx) = setup();
        webcam_tx.send(42)?;
        // and what happen if we receive
        // the data twice? this must return the last value:
        // receive one time
        let value = als.get_raw()?;
        // we receive value...
        assert_eq!(42, value);
        // ...receive two time
        let value = als.get_raw()?;
        // we receive the same value
        assert_eq!(42, value);
        Ok(())
    }

    #[test]
    fn test_get_raw_if_when_sending_twice_a_row_we_receive_well_the_last(
    ) -> Result<(), Box<dyn Error>> {
        let (als, webcam_tx) = setup();
        // and now we send quickly two data
        webcam_tx.send(43)?;
        webcam_tx.send(44)?;
        // ...and we receive just one
        // we got the last data
        let value = als.get_raw()?;
        assert_eq!(44, value);

        Ok(())
    }
}
