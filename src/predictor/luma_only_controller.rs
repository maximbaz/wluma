use crate::frame::capturer::Adjustable;
use std::sync::mpsc::{Receiver, Sender};

pub struct LumaOnlyController {
    prediction_tx: Sender<u64>,
    user_rx: Receiver<u64>,
    last_brightness: Option<u64>,
    luma_to_brightness: Vec<(u8, u64)>,
    pre_reduction_brightness: Option<u64>,
}

impl Adjustable for LumaOnlyController {
    fn adjust(&mut self, luma: u8) {
        let current_brightness = self.user_rx.try_iter().last().or(self.last_brightness);
        let brightness_reduction = self.get_brightness_reduction(luma);

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
            self.last_brightness = current_brightness;

            self.pre_reduction_brightness =
                Some(current_brightness.unwrap() + brightness_reduction);
            self.prediction_tx
                .send(
                    self.pre_reduction_brightness
                        .unwrap()
                        .saturating_sub(brightness_reduction),
                )
                .expect("Unable to send predicted brightness value, channel is dead");
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
            last_brightness: None,
            luma_to_brightness,
            pre_reduction_brightness: None,
        }
    }

    fn get_brightness_reduction(&mut self, luma: u8) -> u64 {
        match self
            .luma_to_brightness
            .iter()
            .find(|&&(luma2, _)| luma <= luma2)
        {
            Some((_, brightness_reduction)) => *brightness_reduction,
            None => 0,
        }
    }
}
