use itertools::Itertools;

use crate::frame::capturer::Adjustable;
use crate::predictor::data::Entry;
use std::{
    collections::HashMap,
    sync::mpsc::{Receiver, Sender},
    time::Duration,
};

use super::{INITIAL_TIMEOUT_SECS, PENDING_COOLDOWN_RESET};

pub struct Controller {
    prediction_tx: Sender<u64>,
    user_rx: Receiver<u64>,
    last_brightness: Option<u64>,
    thresholds: HashMap<u8, u64>,
    pre_reduction_brightness: Option<u64>,
    pending_cooldown: u8,
}

impl Adjustable for Controller {
    fn adjust(&mut self, luma: u8) {
        // TODO
        let lux = "none";

        if self.last_brightness.is_none() {
            // Brightness controller is expected to send the initial value on this channel asap
            let initial_brightness = self
                .user_rx
                .recv_timeout(Duration::from_secs(INITIAL_TIMEOUT_SECS))
                .expect("Did not receive initial brightness value in time");

            self.process_brightness_change(initial_brightness, lux, luma);
        }

        self.process(lux, luma);
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
            pending_cooldown: 0,
        }
    }

    fn process(&mut self, lux: &str, luma: u8) {
        let current_brightness = self
            .user_rx
            .try_iter()
            .last()
            .or(self.last_brightness)
            .expect("Current brightness value must be known by now");

        if self.last_brightness != Some(current_brightness) {
            self.process_brightness_change(current_brightness, lux, luma);
            self.pending_cooldown = PENDING_COOLDOWN_RESET;
        } else if self.pending_cooldown > 0 {
            self.pending_cooldown -= 1;
        } else {
            self.predict(current_brightness, lux, luma);
        }
    }

    fn predict(&mut self, current_brightness: u64, lux: &str, luma: u8) {
        let brightness_reduction = self.get_brightness_reduction(current_brightness, lux, luma);

        let prediction = self
            .pre_reduction_brightness
            .expect("Pre-reduction brightness value must be known by now")
            .saturating_sub(brightness_reduction);

        log::trace!("Prediction: {} (lux: {}, luma: {})", prediction, lux, luma);
        self.prediction_tx
            .send(prediction)
            .expect("Unable to send predicted brightness value, channel is dead");
    }

    fn get_brightness_reduction(&mut self, current_brightness: u64, lux: &str, luma: u8) -> u64 {
        let entries = self
            .thresholds
            .iter()
            .map(|(&luma, &percentage_reduction)| Entry {
                lux: "none".to_string(),
                luma,
                brightness: percentage_reduction,
            })
            .collect_vec();

        let brightness_reduction = self.interpolate(&entries, lux, luma);

        // TODO add test for no curve for current ALS
        (current_brightness as f64 * brightness_reduction.unwrap_or(0) as f64 / 100.) as u64
    }

    fn process_brightness_change(&mut self, new_brightness: u64, lux: &str, luma: u8) {
        let brightness_reduction = self.get_brightness_reduction(new_brightness, lux, luma);
        self.pre_reduction_brightness = Some(new_brightness + brightness_reduction);
        self.last_brightness = Some(new_brightness);
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

        let controller = Controller::new(prediction_tx, user_rx, thresholds);
        Ok((controller, user_tx, prediction_rx))
    }

    #[test]
    fn test_get_brightness_reduction() -> Result<(), Box<dyn Error>> {
        let lux = "none";
        let (mut controller, _, _) = setup()?;

        assert_eq!(controller.get_brightness_reduction(100, lux, 0), 0);
        assert_eq!(controller.get_brightness_reduction(100, lux, 10), 10);
        assert_eq!(controller.get_brightness_reduction(100, lux, 20), 18);
        assert_eq!(controller.get_brightness_reduction(100, lux, 30), 24);
        assert_eq!(controller.get_brightness_reduction(100, lux, 40), 28);
        assert_eq!(controller.get_brightness_reduction(100, lux, 50), 30);
        assert_eq!(controller.get_brightness_reduction(100, lux, 60), 31);
        assert_eq!(controller.get_brightness_reduction(100, lux, 70), 35);
        assert_eq!(controller.get_brightness_reduction(100, lux, 80), 41);
        assert_eq!(controller.get_brightness_reduction(100, lux, 90), 49);
        assert_eq!(controller.get_brightness_reduction(100, lux, 100), 60);

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
