use crate::frame::capturer::Adjustable;
use crate::predictor::data::Entry;
use std::{
    collections::HashMap,
    sync::mpsc::{Receiver, Sender},
};

const COOLDOWN_STEPS: u8 = 15;

pub struct Controller {
    prediction_tx: Sender<u64>,
    user_rx: Receiver<u64>,
    last_brightness: Option<u64>,
    thresholds: HashMap<u8, u64>,
    pre_reduction_brightness: Option<u64>,
    cooldown: u8,
}

impl Adjustable for Controller {
    fn adjust(&mut self, current_luma: u8) {
        log::debug!("");
        log::debug!("current_luma: {:?}", current_luma);

        let current_brightness = self.user_rx.try_iter().last().or(self.last_brightness);
        log::debug!("current_brightness: {:?}", current_brightness);

        let brightness_reduction =
            self.get_brightness_reduction(current_brightness.unwrap(), current_luma);
        log::debug!("brightness_reduction: {:?}", brightness_reduction);

        if self.pre_reduction_brightness.is_none() {
            self.pre_reduction_brightness =
                Some(current_brightness.unwrap() + brightness_reduction);
        }
        log::debug!(
            "pre_reduction_brightness: {:?}",
            self.pre_reduction_brightness
        );

        if self.last_brightness == current_brightness {
            log::debug!(
                "self.last_brightness (= {:?}) == current_brightness",
                self.last_brightness
            );

            if self.cooldown == 0 {
                self.prediction_tx
                    .send(
                        self.pre_reduction_brightness
                            .unwrap()
                            .saturating_sub(brightness_reduction),
                    )
                    .expect("Unable to send predicted brightness value, channel is dead");
            }
        } else {
            log::debug!(
                "self.last_brightness (= {:?}) != current_brightness",
                self.last_brightness
            );

            self.pre_reduction_brightness =
                Some(current_brightness.unwrap() + brightness_reduction);
            self.last_brightness = current_brightness;
            self.cooldown = COOLDOWN_STEPS;
        }

        self.cooldown = self.cooldown.saturating_sub(1);
    }
}

impl Controller {
    pub fn new(
        prediction_tx: Sender<u64>,
        user_rx: Receiver<u64>,
        thresholds: HashMap<u8, u64>,
    ) -> Self {
        Self {
            prediction_tx,
            user_rx,
            last_brightness: None,
            thresholds,
            pre_reduction_brightness: None,
            cooldown: 0,
        }
    }

    fn get_brightness_reduction(&mut self, current_brightness: u64, luma: u8) -> u64 {
        let entries = self
            .thresholds
            .iter()
            .map(|(&luma, &percentage_reduction)| Entry {
                lux: String::default(),
                luma,
                brightness: percentage_reduction, // TODO: Entry.brightness should be renamed to something more generic
            })
            .collect::<Vec<Entry>>();
        let brightness_reduction = self.calculate(entries.iter().collect(), luma);
        (current_brightness as f64 * brightness_reduction as f64 / 100.) as u64
    }
}
