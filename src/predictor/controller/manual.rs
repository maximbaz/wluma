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
        let current_brightness = self.user_rx.try_iter().last().or(self.last_brightness);
        if self.last_brightness.is_none() {
            self.last_brightness = current_brightness;
        }

        let brightness_reduction =
            self.get_brightness_reduction(current_brightness.unwrap(), current_luma);

        if self.pre_reduction_brightness.is_none() {
            self.pre_reduction_brightness =
                Some(current_brightness.unwrap() + brightness_reduction);
        }

        if self.last_brightness == current_brightness {
            if self.cooldown == 0 {
                let prediction = self
                    .pre_reduction_brightness
                    .unwrap()
                    .saturating_sub(brightness_reduction);

                log::trace!(
                    "Prediction: {} (lux: {}, luma: {})",
                    prediction,
                    "none",
                    current_luma
                );
                self.prediction_tx
                    .send(prediction)
                    .expect("Unable to send predicted brightness value, channel is dead");
            }
        } else {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::error::Error;
    use std::sync::mpsc;

    fn setup() -> Result<(Controller, Sender<u64>, Receiver<u64>), Box<dyn Error>> {
        let (user_tx, user_rx) = mpsc::channel();
        let (prediction_tx, prediction_rx) = mpsc::channel();
        let thresholds: HashMap<u8, u64> = [(0, 0), (50, 30), (100, 60)].iter().cloned().collect();

        user_tx.send(0)?;
        let controller = Controller::new(prediction_tx, user_rx, thresholds);
        Ok((controller, user_tx, prediction_rx))
    }

    #[test]
    fn test_get_brightness_reduction() -> Result<(), Box<dyn Error>> {
        let (mut controller, _, _) = setup()?;

        assert_eq!(controller.get_brightness_reduction(100, 0), 0);
        assert_eq!(controller.get_brightness_reduction(100, 10), 10);
        assert_eq!(controller.get_brightness_reduction(100, 20), 18);
        assert_eq!(controller.get_brightness_reduction(100, 30), 24);
        assert_eq!(controller.get_brightness_reduction(100, 40), 28);
        assert_eq!(controller.get_brightness_reduction(100, 50), 30);
        assert_eq!(controller.get_brightness_reduction(100, 60), 31);
        assert_eq!(controller.get_brightness_reduction(100, 70), 35);
        assert_eq!(controller.get_brightness_reduction(100, 80), 41);
        assert_eq!(controller.get_brightness_reduction(100, 90), 49);
        assert_eq!(controller.get_brightness_reduction(100, 100), 60);

        Ok(())
    }

    #[test]
    fn test_change_in_luma() -> Result<(), Box<dyn Error>> {
        let (mut controller, user_tx, prediction_rx) = setup()?;

        user_tx.send(100)?;

        controller.adjust(50);
        assert_eq!(prediction_rx.recv()?, 100);

        controller.adjust(10);
        assert_eq!(prediction_rx.recv()?, 120);

        controller.adjust(80);
        assert_eq!(prediction_rx.recv()?, 89);

        Ok(())
    }

    #[test]
    fn test_change_in_brightness_by_user() -> Result<(), Box<dyn Error>> {
        let (mut controller, user_tx, prediction_rx) = setup()?;

        user_tx.send(100)?;
        controller.adjust(50);
        assert_eq!(prediction_rx.recv()?, 100);

        user_tx.send(123)?;
        controller.adjust(50);
        assert_eq!(prediction_rx.try_recv().is_err(), true);

        user_tx.send(1)?;
        controller.adjust(50);
        assert_eq!(prediction_rx.try_recv().is_err(), true);

        Ok(())
    }
}
