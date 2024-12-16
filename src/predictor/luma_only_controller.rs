use crate::frame::capturer::Adjustable;
use std::sync::mpsc::{Receiver, Sender};

const COOLDOWN_STEPS: u8 = 10;

pub struct LumaOnlyController {
    prediction_tx: Sender<u64>,
    user_rx: Receiver<u64>,
    cooldown: u8,
    last_brightness: Option<u64>,
    luma_to_brightness: Vec<(u8, u64)>,
    pre_reduction_brightness: Option<u64>,
}

impl Adjustable for LumaOnlyController {
    fn adjust(&mut self, luma: u8) {
        log::debug!("luma: {}", luma);

        if self.cooldown > 0 {
            self.cooldown = self.cooldown.saturating_sub(1);
            return;
        }

        let current_brightness = self.user_rx.try_iter().last().or(self.last_brightness);
        let brightness_reduction = self.get_brightness_reduction(current_brightness.unwrap(), luma);

        self.pre_reduction_brightness = self
            .pre_reduction_brightness
            .or(Some(current_brightness.unwrap() + brightness_reduction));

        if self.last_brightness == current_brightness {
            self.prediction_tx
                .send(
                    self.pre_reduction_brightness
                        .unwrap()
                        .saturating_sub(brightness_reduction),
                )
                .expect("Unable to send predicted brightness value, channel is dead");
        } else {
            self.pre_reduction_brightness =
                Some(current_brightness.unwrap() + brightness_reduction);
            self.cooldown = COOLDOWN_STEPS;
            self.last_brightness = current_brightness;
        }
    }
}

impl LumaOnlyController {
    pub fn new(
        prediction_tx: Sender<u64>,
        user_rx: Receiver<u64>,
        luma_to_brightness: Vec<(u8, u64)>,
    ) -> Self {
        Self {
            prediction_tx,
            user_rx,
            cooldown: 0,
            last_brightness: None,
            luma_to_brightness,
            pre_reduction_brightness: None,
        }
    }

    fn get_brightness_reduction(&mut self, current_brightness: u64, luma: u8) -> u64 {
        match self
            .luma_to_brightness
            .iter()
            .find(|&&(luma2, _)| luma <= luma2)
        {
            Some((_, brightness_reduction)) => {
                (current_brightness as f64 * (*brightness_reduction) as f64 / 100.) as u64
            }
            None => 0,
        }
    }
}
