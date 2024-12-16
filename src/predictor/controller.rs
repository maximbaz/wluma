use crate::predictor::data::{Data, Entry};
use itertools::Itertools;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use crate::frame::capturer::Adjustable;

const INITIAL_TIMEOUT_SECS: u64 = 5;
const PENDING_COOLDOWN_RESET: u8 = 15;
const NEXT_ALS_COOLDOWN_RESET: u8 = 15;

pub struct Controller {
    prediction_tx: Sender<u64>,
    user_rx: Receiver<u64>,
    als_rx: Receiver<String>,
    pending_cooldown: u8,
    pending: Option<Entry>,
    data: Data,
    stateful: bool,
    initial_brightness: Option<u64>,
    last_als: Option<String>,
    next_als: Option<String>,
    next_als_cooldown: u8,
    output_name: String,
}

impl Adjustable for Controller {
    fn adjust(&mut self, luma: u8) {
        if self.last_als.is_none() {
            // ALS controller is expected to send the initial value on this channel asap
            self.last_als = self
                .als_rx
                .recv_timeout(Duration::from_secs(INITIAL_TIMEOUT_SECS))
                .map_or_else(
                    |_| panic!("Did not receive initial ALS value in time"),
                    Some,
                );

            // Brightness controller is expected to send the initial value on this channel asap
            let initial_brightness = self
                .user_rx
                .recv_timeout(Duration::from_secs(INITIAL_TIMEOUT_SECS))
                .map_or_else(
                    |_| panic!("Did not receive initial brightness value in time"),
                    Some,
                );

            // If there are no learned entries yet, we will use this as the first data point,
            // assuming that user is happy with the current brightness settings
            if self.data.entries.is_empty() {
                self.initial_brightness = initial_brightness;
            };
        }

        match self.als_rx.try_iter().last() {
            new_als @ Some(_) if self.next_als != new_als => {
                self.next_als = new_als;
                self.next_als_cooldown = NEXT_ALS_COOLDOWN_RESET;
            }
            _ if self.next_als_cooldown > 1 => {
                self.next_als_cooldown -= 1;
            }
            _ if self.next_als_cooldown == 1 => {
                self.next_als_cooldown = 0;
                self.last_als = self.next_als.take();
            }
            _ => {}
        }

        let lux = &self.last_als.clone().expect("ALS value must be known");
        self.process(lux, luma);
    }
}

impl Controller {
    pub fn new(
        prediction_tx: Sender<u64>,
        user_rx: Receiver<u64>,
        als_rx: Receiver<String>,
        stateful: bool,
        output_name: &str,
    ) -> Self {
        let data = if stateful {
            Data::load(output_name)
        } else {
            Data::new(output_name)
        };

        Self {
            prediction_tx,
            user_rx,
            als_rx,
            pending_cooldown: 0,
            pending: None,
            data,
            stateful,
            initial_brightness: None,
            last_als: None,
            next_als: None,
            next_als_cooldown: 0,
            output_name: output_name.to_string(),
        }
    }

    fn process(&mut self, lux: &str, luma: u8) {
        let initial_brightness = self.initial_brightness.take();
        let user_changed_brightness = self.user_rx.try_iter().last().or(initial_brightness);

        if let Some(brightness) = user_changed_brightness {
            self.pending = match &self.pending {
                // First time we notice user adjusting brightness, freeze lux and luma...
                None => Some(Entry::new(lux, luma, brightness)),
                // ... but as user keeps changing brightness,
                // allow some time for them to reach the desired brightness level for the pending lux and luma
                Some(Entry { lux, luma, .. }) => Some(Entry::new(lux, *luma, brightness)),
            };
            // Every time user changed brightness, reset the cooldown period
            self.pending_cooldown = PENDING_COOLDOWN_RESET;
        } else if self.pending_cooldown > 0 {
            self.pending_cooldown -= 1;
        } else if self.pending.is_some() {
            self.learn();
        } else {
            self.predict(lux, luma);
        }
    }

    fn learn(&mut self) {
        let pending = self.pending.take().expect("No pending entry to learn");
        log::debug!("[{}] Learning {:?}", self.output_name, pending);

        self.data.entries.retain(|entry| {
            let different_env = entry.lux != pending.lux;

            let same_env_darker_screen = entry.lux == pending.lux
                && entry.luma < pending.luma
                && entry.brightness >= pending.brightness;

            let same_env_brighter_screen = entry.lux == pending.lux
                && entry.luma > pending.luma
                && entry.brightness <= pending.brightness;

            different_env || same_env_darker_screen || same_env_brighter_screen
        });

        self.data.entries.push(pending);

        self.data
            .entries
            .sort_unstable_by(|x, y| x.lux.cmp(&y.lux).then(x.luma.cmp(&y.luma)));

        if self.stateful {
            self.data.save().expect("Unable to save data");
        }
    }

    fn predict(&mut self, lux: &str, luma: u8) {
        let entries = self
            .data
            .entries
            .iter()
            .filter(|e| e.lux == lux)
            .collect_vec();

        if entries.is_empty() {
            return;
        }

        let points = entries
            .iter()
            .map(|entry| {
                let distance = (luma as f64 - entry.luma as f64).abs();
                (entry.brightness as f64, distance)
            })
            .collect_vec();

        let points = points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let other_distances: f64 = points[0..i]
                    .iter()
                    .chain(&points[i + 1..])
                    .map(|p| p.1)
                    .product();
                (p.0, p.1, other_distances)
            })
            .collect_vec();

        let distance_denominator: f64 = points
            .iter()
            .map(|p| p.1)
            .combinations(points.len() - 1)
            .map(|c| c.iter().product::<f64>())
            .sum();

        let prediction = points
            .iter()
            .map(|p| p.0 * p.2 / distance_denominator)
            .sum::<f64>() as u64;

        log::trace!("Prediction: {} (lux: {}, luma: {})", prediction, lux, luma);
        self.prediction_tx
            .send(prediction)
            .expect("Unable to send predicted brightness value, channel is dead");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use itertools::iproduct;
    use std::collections::HashSet;
    use std::error::Error;
    use std::sync::mpsc;

    const ALS_DARK: &str = "dark";
    const ALS_DIM: &str = "dim";
    const ALS_BRIGHT: &str = "bright";

    fn setup() -> Result<(Controller, Sender<u64>, Receiver<u64>), Box<dyn Error>> {
        let (als_tx, als_rx) = mpsc::channel();
        let (user_tx, user_rx) = mpsc::channel();
        let (prediction_tx, prediction_rx) = mpsc::channel();
        als_tx.send(ALS_BRIGHT.to_string())?;
        user_tx.send(0)?;
        let controller = Controller::new(prediction_tx, user_rx, als_rx, false, "Dell 1");
        Ok((controller, user_tx, prediction_rx))
    }

    #[test]
    fn test_process_first_user_change() -> Result<(), Box<dyn Error>> {
        let (mut controller, user_tx, _) = setup()?;

        // User changes brightness to value 33 for a given lux and luma
        user_tx.send(33)?;
        controller.process(ALS_DIM, 66);

        assert_eq!(Some(Entry::new(ALS_DIM, 66, 33)), controller.pending);
        assert_eq!(PENDING_COOLDOWN_RESET, controller.pending_cooldown);

        Ok(())
    }

    #[test]
    fn test_process_several_continuous_user_changes() -> Result<(), Box<dyn Error>> {
        let (mut controller, user_tx, _) = setup()?;

        // User initiates brightness change for a given lux and luma to value 33...
        user_tx.send(33)?;
        controller.process(ALS_DIM, 66);
        // then quickly continues increasing it to 34 (while lux and luma might already be different)...
        user_tx.send(34)?;
        controller.process(ALS_BRIGHT, 36);
        // and even faster to 36 (which is the indended brightness value they wish to learn for the initial lux and luma)
        user_tx.send(35)?;
        user_tx.send(36)?;
        controller.process(ALS_DARK, 16);

        assert_eq!(Some(Entry::new(ALS_DIM, 66, 36)), controller.pending);
        assert_eq!(PENDING_COOLDOWN_RESET, controller.pending_cooldown);

        Ok(())
    }

    #[test]
    fn test_process_learns_user_change_after_cooldown() -> Result<(), Box<dyn Error>> {
        let (mut controller, user_tx, _) = setup()?;

        // User changes brightness to a desired value
        user_tx.send(33)?;
        controller.process(ALS_DIM, 66);
        user_tx.send(33)?;
        controller.process(ALS_BRIGHT, 36);
        user_tx.send(35)?;
        controller.process(ALS_DARK, 16);

        for i in 1..=PENDING_COOLDOWN_RESET {
            // User doesn't change brightness anymore, so even if lux or luma change, we are in cooldown period
            controller.process(ALS_BRIGHT, i);
            assert_eq!(PENDING_COOLDOWN_RESET - i, controller.pending_cooldown);
            assert_eq!(Some(Entry::new(ALS_DIM, 66, 35)), controller.pending);
        }

        // One final process will trigger the learning
        controller.process(ALS_DARK, 17);

        assert_eq!(None, controller.pending);
        assert_eq!(0, controller.pending_cooldown);
        assert_eq!(vec![Entry::new(ALS_DIM, 66, 35)], controller.data.entries);

        Ok(())
    }

    // If user configured brightness value in certain conditions (amount of light around, screen contents),
    // how changes in environment or screen contents can affect the desired brightness level:
    //
    // |                 | darker env      | same env         | brighter env     |
    // | darker screen   | any             | same or brighter | same or brighter |
    // | same screen     | same or dimmer  | only same        | same or brighter |
    // | brighter screen | same or dimmer  | same or dimmer   | any              |
    //
    // *UPDATE*: experimenting with not changing other envs

    #[test]
    fn test_learn_data_cleanup() -> Result<(), Box<dyn Error>> {
        let (mut controller, _, _) = setup()?;

        let pending = Entry::new(ALS_DIM, 20, 30);

        let all_als = vec![ALS_DARK, ALS_DIM, ALS_BRIGHT];
        let all_combinations: HashSet<_> = iproduct!(-1i32..=1, -1i32..=1, -1i32..=1)
            .map(|(i, j, k)| Entry::new(all_als[(1 + i) as usize], (20 + j) as u8, (30 + k) as u64))
            .collect();

        let to_be_deleted: HashSet<_> = vec![
            // same env darker screen
            Entry::new(ALS_DIM, 19, 29),
            // same env same screen
            Entry::new(ALS_DIM, 20, 29),
            Entry::new(ALS_DIM, 20, 31),
            // same env brighter screen
            Entry::new(ALS_DIM, 21, 31),
        ]
        .into_iter()
        .collect();

        controller.data.entries = all_combinations.iter().cloned().collect_vec();
        controller.pending = Some(pending);

        controller.learn();

        let to_remain: HashSet<_> = all_combinations.difference(&to_be_deleted).collect();
        let remained = controller.data.entries.iter().collect();

        assert_eq!(
            Vec::<&&Entry>::new(),
            to_remain.difference(&remained).collect_vec(),
            "unexpected entries were removed"
        );

        assert_eq!(
            Vec::<&&Entry>::new(),
            remained.difference(&to_remain).collect_vec(),
            "some entries were not removed"
        );

        assert_eq!(
            to_remain.len(),
            controller.data.entries.len(),
            "duplicate entries remained"
        );

        Ok(())
    }

    #[test]
    fn test_predict_no_data_points() -> Result<(), Box<dyn Error>> {
        let (mut controller, _, prediction_rx) = setup()?;
        controller.data.entries = vec![];

        // predict() should not be called with no data, but just in case confirm we don't panic
        controller.predict(ALS_DIM, 20);

        assert_eq!(true, prediction_rx.try_recv().is_err());

        Ok(())
    }

    #[test]
    fn test_predict_no_data_points_for_current_als_profile() -> Result<(), Box<dyn Error>> {
        let (mut controller, _, prediction_rx) = setup()?;
        controller.data.entries = vec![
            Entry::new(ALS_DARK, 50, 100),
            Entry::new(ALS_BRIGHT, 60, 100),
        ];

        // predict() should not be called with no data, but just in case confirm we don't panic
        controller.predict(ALS_DIM, 20);

        assert_eq!(true, prediction_rx.try_recv().is_err());

        Ok(())
    }

    #[test]
    fn test_predict_one_data_point() -> Result<(), Box<dyn Error>> {
        let (mut controller, _, prediction_rx) = setup()?;
        controller.data.entries = vec![Entry::new(ALS_DIM, 10, 15)];

        controller.predict(ALS_DIM, 20);

        assert_eq!(15, prediction_rx.try_recv()?);
        Ok(())
    }

    #[test]
    fn test_predict_known_conditions() -> Result<(), Box<dyn Error>> {
        let (mut controller, _, prediction_rx) = setup()?;
        controller.data.entries = vec![Entry::new(ALS_DIM, 10, 15), Entry::new(ALS_DIM, 20, 30)];

        controller.predict(ALS_DIM, 20);

        assert_eq!(30, prediction_rx.try_recv()?);
        Ok(())
    }

    #[test]
    fn test_predict_approximate() -> Result<(), Box<dyn Error>> {
        let (mut controller, _, prediction_rx) = setup()?;
        controller.data.entries = vec![
            Entry::new(ALS_DIM, 10, 15),
            Entry::new(ALS_DIM, 20, 30),
            Entry::new(ALS_DIM, 100, 100),
        ];

        // Approximated using weighted distance to all known points:
        // dist1 = sqrt((x1 - x2)^2 + (y1 - y2)^2)
        // weight1 = (1/dist1) / (1/dist1 + 1/dist2 + 1/dist3)
        // prediction = weight1*brightness1 + weight2*brightness2 + weight3*brightness
        controller.predict(ALS_DIM, 50);

        assert_eq!(43, prediction_rx.try_recv()?);
        Ok(())
    }

    #[test]
    fn test_predict_only_uses_data_for_current_als_profile() -> Result<(), Box<dyn Error>> {
        let (mut controller, _, prediction_rx) = setup()?;
        controller.data.entries = vec![
            Entry::new(ALS_DIM, 10, 15),
            Entry::new(ALS_DIM, 20, 30),
            Entry::new(ALS_DIM, 100, 100),
            Entry::new(ALS_DARK, 50, 100),
            Entry::new(ALS_BRIGHT, 51, 100),
        ];

        controller.predict(ALS_DIM, 50);

        assert_eq!(43, prediction_rx.try_recv()?);
        Ok(())
    }
}
