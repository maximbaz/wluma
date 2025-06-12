use super::{INITIAL_TIMEOUT_SECS, NEXT_ALS_COOLDOWN_RESET, PENDING_COOLDOWN_RESET};
use crate::{channel_ext::ReceiverExt, predictor::data::Entry};
use itertools::Itertools;
use smol::channel::{Receiver, Sender};
use std::{collections::HashMap, time::Duration};

pub struct Controller {
    prediction_tx: Sender<u64>,
    user_rx: Receiver<u64>,
    als_rx: Receiver<String>,
    last_brightness: Option<u64>,
    thresholds: HashMap<String, HashMap<u8, u64>>,
    pre_reduction_brightness: Option<u64>,
    pending_cooldown: u8,
    last_als: Option<String>,
    next_als: Option<String>,
    next_als_cooldown: u8,
    output_name: String,
}

impl Controller {
    pub fn new(
        prediction_tx: Sender<u64>,
        user_rx: Receiver<u64>,
        als_rx: Receiver<String>,
        thresholds: HashMap<String, HashMap<u8, u64>>,
        output_name: &str,
    ) -> Self {
        Self {
            prediction_tx,
            user_rx,
            als_rx,
            last_brightness: None,
            thresholds,
            pre_reduction_brightness: None,
            pending_cooldown: 0,
            last_als: None,
            next_als: None,
            next_als_cooldown: 0,
            output_name: output_name.to_string(),
        }
    }

    pub async fn adjust(&mut self, luma: u8) {
        if self.last_als.is_none() {
            // ALS controller is expected to send the initial value on this channel asap
            self.last_als = Some(
                self.als_rx
                    .recv_or_panic_after_timeout(Duration::from_secs(INITIAL_TIMEOUT_SECS))
                    .await
                    .expect("als_rx closed unexpectedly"),
            );
        }

        match self
            .als_rx
            .recv_maybe_last()
            .await
            .expect("als_rx closed unexpectedly")
        {
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

        self.process(lux, luma).await;
    }

    async fn process(&mut self, lux: &str, luma: u8) {
        if self.last_brightness.is_none() {
            // Brightness controller is expected to send the initial value on this channel asap
            self.last_brightness = self
                .user_rx
                .recv_maybe_last()
                .await
                .expect("user_rx closed unexpectedly")
                .or_else(|| panic!("Did not receive initial brightness value"));

            self.process_brightness_change(self.last_brightness.unwrap(), lux, luma);
        }

        let current_brightness = self
            .user_rx
            .recv_maybe_last()
            .await
            .expect("user_rx closed unexpectedly")
            .or(self.last_brightness)
            .expect("Current brightness value must be known by now");

        if self.last_brightness != Some(current_brightness) {
            self.process_brightness_change(current_brightness, lux, luma);
            self.pending_cooldown = PENDING_COOLDOWN_RESET;
        } else if self.pending_cooldown > 0 {
            self.pending_cooldown -= 1;
        } else {
            self.predict(current_brightness, lux, luma).await;
        }
    }

    async fn predict(&mut self, current_brightness: u64, lux: &str, luma: u8) {
        let brightness_reduction = self.get_brightness_reduction(current_brightness, lux, luma);

        let prediction = self
            .pre_reduction_brightness
            .expect("Pre-reduction brightness value must be known by now")
            .saturating_sub(brightness_reduction);

        log::trace!(
            "[{}] Prediction: {prediction} (lux: {lux}, luma: {luma})",
            self.output_name
        );
        self.prediction_tx
            .send(prediction)
            .await
            .expect("Unable to send predicted brightness value, channel is dead");
    }

    fn get_brightness_reduction(&mut self, current_brightness: u64, lux: &str, luma: u8) -> u64 {
        let entries = self
            .thresholds
            .get(lux)
            .unwrap_or(&HashMap::new())
            .iter()
            .map(|(&luma, &percentage_reduction)| Entry {
                lux: lux.to_string(),
                luma,
                brightness: percentage_reduction,
            })
            .collect_vec();

        let brightness_reduction = super::interpolate(&entries, lux, luma);

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
    use crate::ErrorBox;

    use super::*;
    use macro_rules_attribute::apply;
    use smol::channel;
    use smol_macros::test;
    use std::collections::HashMap;

    const ALS_UNKNOWN: &str = "not-configured-threshold";
    const ALS_DIM: &str = "dim";

    async fn setup() -> Result<(Controller, Sender<u64>, Receiver<u64>), ErrorBox> {
        let (als_tx, als_rx) = channel::bounded(128);
        let (user_tx, user_rx) = channel::bounded(128);
        let (prediction_tx, prediction_rx) = channel::bounded(128);
        als_tx.send(ALS_DIM.to_string()).await?;
        user_tx.send(0).await?;

        let thresholds: HashMap<String, HashMap<u8, u64>> = [(
            ALS_DIM.to_string(),
            [(0, 0), (50, 30), (100, 60)].into_iter().collect(),
        )]
        .into_iter()
        .collect();

        let controller = Controller::new(prediction_tx, user_rx, als_rx, thresholds, "eDP-1");
        Ok((controller, user_tx, prediction_rx))
    }

    #[apply(test!)]
    async fn test_get_brightness_reduction() -> Result<(), ErrorBox> {
        let (mut controller, _, _) = setup().await?;

        assert_eq!(controller.get_brightness_reduction(100, ALS_DIM, 0), 0);
        assert_eq!(controller.get_brightness_reduction(100, ALS_DIM, 10), 10);
        assert_eq!(controller.get_brightness_reduction(100, ALS_DIM, 20), 18);
        assert_eq!(controller.get_brightness_reduction(100, ALS_DIM, 30), 24);
        assert_eq!(controller.get_brightness_reduction(100, ALS_DIM, 40), 28);
        assert_eq!(controller.get_brightness_reduction(100, ALS_DIM, 50), 30);
        assert_eq!(controller.get_brightness_reduction(100, ALS_DIM, 60), 31);
        assert_eq!(controller.get_brightness_reduction(100, ALS_DIM, 70), 35);
        assert_eq!(controller.get_brightness_reduction(100, ALS_DIM, 80), 41);
        assert_eq!(controller.get_brightness_reduction(100, ALS_DIM, 90), 49);
        assert_eq!(controller.get_brightness_reduction(100, ALS_DIM, 100), 60);

        Ok(())
    }

    #[apply(test!)]
    async fn test_no_brightness_reduction_for_not_configured_als_threshold() -> Result<(), ErrorBox>
    {
        let (mut controller, _, _) = setup().await?;

        assert_eq!(controller.get_brightness_reduction(100, ALS_UNKNOWN, 0), 0);
        assert_eq!(controller.get_brightness_reduction(100, ALS_UNKNOWN, 10), 0);
        assert_eq!(controller.get_brightness_reduction(100, ALS_UNKNOWN, 20), 0);
        assert_eq!(controller.get_brightness_reduction(100, ALS_UNKNOWN, 30), 0);
        assert_eq!(controller.get_brightness_reduction(100, ALS_UNKNOWN, 40), 0);
        assert_eq!(controller.get_brightness_reduction(100, ALS_UNKNOWN, 50), 0);
        assert_eq!(controller.get_brightness_reduction(100, ALS_UNKNOWN, 60), 0);
        assert_eq!(controller.get_brightness_reduction(100, ALS_UNKNOWN, 70), 0);
        assert_eq!(controller.get_brightness_reduction(100, ALS_UNKNOWN, 80), 0);
        assert_eq!(controller.get_brightness_reduction(100, ALS_UNKNOWN, 90), 0);
        assert_eq!(
            controller.get_brightness_reduction(100, ALS_UNKNOWN, 100),
            0
        );

        Ok(())
    }

    #[apply(test!)]
    async fn test_change_in_luma() -> Result<(), ErrorBox> {
        let (mut controller, user_tx, prediction_rx) = setup().await?;

        user_tx.send(100).await?;

        controller.process(ALS_DIM, 50).await;
        assert_eq!(prediction_rx.recv().await?, 100);

        controller.process(ALS_DIM, 10).await;
        assert_eq!(prediction_rx.recv().await?, 120);

        controller.process(ALS_DIM, 80).await;
        assert_eq!(prediction_rx.recv().await?, 89);

        Ok(())
    }

    #[apply(test!)]
    async fn test_change_in_brightness_by_user() -> Result<(), ErrorBox> {
        let (mut controller, user_tx, prediction_rx) = setup().await?;

        // Initial brightness is used to predict right away
        user_tx.send(100).await?;
        controller.process(ALS_DIM, 50).await;
        assert_eq!(prediction_rx.recv().await?, 100);

        // Consequent user change causes prediction only after cooldown
        user_tx.send(123).await?;
        for i in 0..=PENDING_COOLDOWN_RESET {
            // User doesn't change brightness anymore, so even if lux or luma change, we are in cooldown period
            controller.process(ALS_DIM, i).await;
            assert_eq!(PENDING_COOLDOWN_RESET - i, controller.pending_cooldown);
            assert!(prediction_rx.is_empty());
        }

        // One final call will generate the actual prediction
        controller.process(ALS_DIM, 50).await;
        assert_eq!(0, controller.pending_cooldown);
        assert_eq!(87, prediction_rx.recv().await?);

        Ok(())
    }
}
