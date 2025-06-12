use crate::channel_ext::ReceiverExt;
use crate::frame::compute_perceived_lightness_percent;
use crate::ErrorBox;
use itertools::Itertools;
use smol::channel::{Receiver, Sender};
use smol::lock::Mutex;
use std::collections::HashMap;
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
    webcam_tx: Sender<u64>,
    video: usize,
}

impl Webcam {
    pub fn new(webcam_tx: Sender<u64>, video: usize) -> Self {
        Self { webcam_tx, video }
    }

    pub fn run(&mut self) {
        loop {
            self.step();
        }
    }

    fn step(&mut self) {
        if let Ok((rgbs, pixels)) = self.frame() {
            let lux = compute_perceived_lightness_percent(&rgbs, false, pixels) as u64;

            self.webcam_tx
                .send_blocking(lux) // TODO: async
                .expect("Unable to send new webcam lux value, channel is dead");
        };

        thread::sleep(Duration::from_millis(WAITING_SLEEP_MS));
    }

    fn frame(&mut self) -> Result<(Vec<u8>, usize), ErrorBox> {
        let (device, pixels) = Self::setup(self.video)?;
        let mut stream = Stream::new(&device, Type::VideoCapture)?;
        let (rgbs, _) = stream.next()?;

        Ok((rgbs.to_vec(), pixels))
    }

    fn setup(video: usize) -> Result<(Device, usize), ErrorBox> {
        let device = Device::new(video)?;
        let mut format = device.format()?;
        format.fourcc = FourCC::new(b"RGB3");
        let (width, height) = device
            .enum_framesizes(format.fourcc)?
            .into_iter()
            .flat_map(|f| {
                f.size
                    .to_discrete()
                    .into_iter()
                    .map(|d| (d.width, d.height))
                    .collect_vec()
            })
            .min_by(|&(w1, h1), &(w2, h2)| h1.cmp(&h2).then(w1.cmp(&w2)))
            .ok_or("Unable to find minimum resolution")?;

        format.height = height;
        format.width = width;
        device.set_format(&format)?;

        Ok((device, width as usize * height as usize))
    }
}

pub struct Als {
    webcam_rx: Receiver<u64>,
    thresholds: HashMap<u64, String>,
    lux: Mutex<u64>,
}

impl Als {
    pub fn new(webcam_rx: Receiver<u64>, thresholds: HashMap<u64, String>) -> Self {
        Self {
            webcam_rx,
            thresholds,
            lux: Mutex::new(DEFAULT_LUX),
        }
    }

    pub async fn get(&self) -> Result<String, ErrorBox> {
        let raw = self.get_raw().await?;
        let profile = super::find_profile(raw, &self.thresholds);

        log::trace!("ALS (webcam): {} ({})", profile, raw);
        Ok(profile)
    }

    async fn get_raw(&self) -> Result<u64, ErrorBox> {
        let new_value = self
            .webcam_rx
            .recv_maybe_last()
            .await
            .expect("webcam_rx closed unexpectedly")
            .unwrap_or(*self.lux.lock_blocking());
        *self.lux.lock_blocking() = new_value;
        Ok(new_value)
    }
}

#[cfg(test)]
mod tests {
    use macro_rules_attribute::apply;
    use smol::channel;
    use smol_macros::test;

    use super::*;

    async fn setup() -> (Als, Sender<u64>) {
        let (webcam_tx, webcam_rx) = channel::bounded(128);
        let als = Als::new(webcam_rx, HashMap::default());
        (als, webcam_tx)
    }

    #[apply(test!)]
    async fn test_get_raw_returns_default_value_when_no_data_from_webcam() -> Result<(), ErrorBox> {
        let (als, _) = setup().await;

        assert_eq!(DEFAULT_LUX, als.get_raw().await?);
        Ok(())
    }

    #[apply(test!)]
    async fn test_get_raw_returns_value_from_webcam() -> Result<(), ErrorBox> {
        let (als, webcam_tx) = setup().await;

        webcam_tx.send(42).await?;

        assert_eq!(42, als.get_raw().await?);
        Ok(())
    }

    #[apply(test!)]
    async fn test_get_raw_returns_most_recent_value_from_webcam() -> Result<(), ErrorBox> {
        let (als, webcam_tx) = setup().await;

        webcam_tx.send(42).await?;
        webcam_tx.send(43).await?;
        webcam_tx.send(44).await?;

        assert_eq!(44, als.get_raw().await?);
        Ok(())
    }

    #[apply(test!)]
    async fn test_get_raw_returns_last_known_value_from_webcam_when_no_new_data(
    ) -> Result<(), ErrorBox> {
        let (als, webcam_tx) = setup().await;

        webcam_tx.send(42).await?;
        webcam_tx.send(43).await?;

        assert_eq!(43, als.get_raw().await?);
        assert_eq!(43, als.get_raw().await?);
        assert_eq!(43, als.get_raw().await?);
        Ok(())
    }
}
