use super::Brightness;
use std::error::Error;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::Duration;

const TRANSITION_MAX_MS: u64 = 200;
const TRANSITION_STEP_MS: u64 = 1;
const WAITING_SLEEP_MS: u64 = 100;

pub struct Controller {
    brightness: Box<dyn Brightness>,
    user_tx: Sender<u64>,
    prediction_rx: Receiver<u64>,
    current: u64,
    target: Option<Target>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct Target {
    desired: u64,
    step: i64,
}

impl Target {
    fn reached(&self, current: u64) -> bool {
        (self.step > 0 && current >= self.desired) || (self.step < 0 && current <= self.desired)
    }
}

impl Controller {
    pub fn new(
        brightness: Box<dyn Brightness>,
        user_tx: Sender<u64>,
        prediction_rx: Receiver<u64>,
    ) -> Self {
        Self {
            brightness,
            user_tx,
            prediction_rx,
            current: 0,
            target: None,
        }
    }

    pub fn run(&mut self) -> Result<(), Box<dyn Error>> {
        loop {
            self.step();
        }
    }

    fn step(&mut self) {
        match self.brightness.get() {
            Ok(new_brightness) => {
                // 1. check if user wants to learn a new value - this overrides any ongoing activity
                if new_brightness != self.current {
                    return self.update_current(new_brightness);
                }

                // 2. check if predictor wants to set a new value
                if let Some(desired) = self.prediction_rx.try_iter().last() {
                    self.update_target(desired);
                }

                // 3. continue the transition if there is one in progress
                if self.target.is_some() {
                    return self.transition();
                }
            }
            Err(err) => log::error!("Unable to get brightness value: {:?}", err),
        };

        // 4. nothing to do, sleep and check again
        thread::sleep(Duration::from_millis(WAITING_SLEEP_MS));
    }

    fn update_current(&mut self, new_brightness: u64) {
        self.current = new_brightness;
        self.user_tx
            .send(new_brightness)
            .expect("Unable to send new brightness value set by user, channel is dead");
        self.target = None;
    }

    fn update_target(&mut self, desired: u64) {
        match &self.target {
            Some(old_target) if old_target.desired == desired => (),
            _ if desired == self.current => (),
            _ => {
                let step = if desired > self.current {
                    div_ceil(desired - self.current, TRANSITION_MAX_MS)
                } else {
                    -div_ceil(self.current - desired, TRANSITION_MAX_MS)
                };
                self.target = Some(Target { desired, step });
            }
        };
    }

    fn transition(&mut self) {
        if let Some(target) = &self.target {
            if target.reached(self.current) {
                self.target = None;
            } else {
                let new_value = (self.current as i64 + target.step).max(0) as u64;
                match self.brightness.set(new_value) {
                    Ok(current) => self.current = current,
                    Err(err) => log::error!(
                        "Unable to set brightness to value '{}': {:?}",
                        new_value,
                        err
                    ),
                };
                thread::sleep(Duration::from_millis(TRANSITION_STEP_MS));
            }
        }
    }
}

fn div_ceil(x: u64, y: u64) -> i64 {
    ((x + y - 1) / y) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brightness::MockBrightness;
    use mockall::predicate;
    use std::sync::mpsc;

    // Intentionally not in main code to prevent confusing fields by accident
    fn target(desired: u64, step: i64) -> Target {
        Target { desired, step }
    }

    fn setup(brightness_mock: MockBrightness) -> (Controller, Sender<u64>, Receiver<u64>) {
        let (user_tx, user_rx) = mpsc::channel();
        let (prediction_tx, prediction_rx) = mpsc::channel();
        let controller = Controller::new(Box::new(brightness_mock), user_tx, prediction_rx);
        (controller, prediction_tx, user_rx)
    }

    #[test]
    fn test_step_first_run() -> Result<(), Box<dyn Error>> {
        let mut brightness_mock = MockBrightness::new();
        brightness_mock.expect_get().return_once(|| Ok(42));
        let (mut controller, prediction_tx, user_rx) = setup(brightness_mock);

        // even if predictor already wants a change...
        prediction_tx.send(37)?;

        // when we execute the first step...
        controller.step();

        // a real current brightness level is respected and sent to predictor
        assert_eq!(42, controller.current);
        assert_eq!(42, user_rx.try_recv()?);
        assert_eq!(true, controller.target.is_none());

        Ok(())
    }

    #[test]
    fn test_step_user_changed_brightness() -> Result<(), Box<dyn Error>> {
        let mut brightness_mock = MockBrightness::new();
        brightness_mock.expect_get().return_once(|| Ok(42));
        let (mut controller, prediction_tx, user_rx) = setup(brightness_mock);

        // when last brightness differs from the current one
        controller.current = 66;

        // even if predictor wants a change...
        prediction_tx.send(37)?;

        // ... or we were already in a transition
        controller.target = Some(target(77, 1));

        // when we execute the next step...
        controller.step();

        // we notice a change in brightness made by user and that takes priority
        assert_eq!(42, controller.current);
        assert_eq!(42, user_rx.try_recv()?);
        assert_eq!(true, controller.target.is_none());

        Ok(())
    }

    #[test]
    fn test_update_target_ignore_when_desired_didnt_change() {
        let old_target = Some(target(10, -20));
        let (mut controller, _, _) = setup(MockBrightness::new());
        controller.target = old_target;
        controller.current = 7;

        controller.update_target(10);

        assert_eq!(old_target, controller.target);
    }

    #[test]
    fn test_update_target_ignore_when_desired_equals_current() {
        let old_target = Some(target(10, -20));
        let (mut controller, _, _) = setup(MockBrightness::new());
        controller.target = old_target;
        controller.current = 7;

        controller.update_target(7);

        assert_eq!(old_target, controller.target);
    }

    #[test]
    fn test_update_target_finds_minimal_step_that_reaches_target_within_transition_duration() {
        let (mut controller, _, _) = setup(MockBrightness::new());
        controller.current = 10000;

        let test_cases = vec![
            (10001, 1),
            (10013, 1),
            (10199, 1),
            (10200, 1),
            (10413, 3),
            (11732, 9),
            (9999, -1),
            (9983, -1),
            (9801, -1),
            (9800, -1),
            (9473, -3),
            (8433, -8),
        ];

        for (desired, expected_step) in test_cases {
            controller.update_target(desired);
            assert_eq!(Some(target(desired, expected_step)), controller.target);
        }
    }

    #[test]
    fn test_transition_reset_target_when_reached() {
        let (mut controller, _, _) = setup(MockBrightness::new());
        controller.current = 10;
        controller.target = Some(target(10, 20));

        controller.transition();

        assert_eq!(None, controller.target);
    }

    #[test]
    fn test_transition_increases_brightness_with_next_step() {
        let mut brightness_mock = MockBrightness::new();
        brightness_mock
            .expect_set()
            .with(predicate::eq(12))
            .times(1)
            .returning(Ok);
        let (mut controller, _, _) = setup(brightness_mock);
        controller.current = 10;
        controller.target = Some(target(20, 2));

        controller.transition();

        assert_eq!(12, controller.current);
    }

    #[test]
    fn test_transition_decreases_brightness_with_next_step() {
        let mut brightness_mock = MockBrightness::new();
        brightness_mock
            .expect_set()
            .with(predicate::eq(9))
            .times(1)
            .returning(Ok);
        let (mut controller, _, _) = setup(brightness_mock);
        controller.current = 10;
        controller.target = Some(target(9, -1));

        controller.transition();

        assert_eq!(9, controller.current);
    }

    #[test]
    fn test_transition_doesnt_decrease_below_0() {
        let mut brightness_mock = MockBrightness::new();
        brightness_mock
            .expect_set()
            .with(predicate::eq(0))
            .times(1)
            .returning(Ok);
        let (mut controller, _, _) = setup(brightness_mock);
        controller.current = 1;
        controller.target = Some(target(0, -2)); // step of -2 should not overshoot

        controller.transition();

        assert_eq!(0, controller.current);
    }

    #[test]
    fn test_target_reached() {
        assert_eq!(false, target(10, 1).reached(9));
        assert_eq!(true, target(10, 1).reached(10));
        assert_eq!(true, target(10, 1).reached(11));

        assert_eq!(true, target(10, -1).reached(9));
        assert_eq!(true, target(10, -1).reached(10));
        assert_eq!(false, target(10, -1).reached(11));
    }
}
