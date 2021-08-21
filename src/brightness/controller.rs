use super::Brightness;
use std::error::Error;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::Duration;

const TRANSITION_SPEED_MS: u64 = 200;
const SLEEP_MS: u64 = 100;

pub struct Controller {
    brightness: Box<dyn Brightness>,
    user_tx: Sender<u64>,
    prediction_rx: Receiver<u64>,
    current: u64,
    target: Option<Target>,
}

struct Target {
    desired: u64,
    step: i64,
    sleep: u64,
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
            self.step()?;
        }
    }

    fn step(&mut self) -> Result<(), Box<dyn Error>> {
        // 1. check if user wants to learn a new value - this overrides any ongoing activity
        let new_brightness = self.brightness.get()?;
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

        // 4. nothing to do, sleep and check again
        thread::sleep(Duration::from_millis(SLEEP_MS));
        Ok(())
    }

    fn update_current(&mut self, new_brightness: u64) -> Result<(), Box<dyn Error>> {
        self.current = new_brightness;
        self.user_tx.send(new_brightness)?;
        self.target = None;
        Ok(())
    }

    fn update_target(&mut self, desired: u64) {
        match &self.target {
            Some(old_target) if old_target.desired == desired => (),
            _ if desired == self.current => (),
            _ => {
                let diff = abs_diff(desired, self.current);
                let dir = if self.current < desired { 1 } else { -1 };
                let (step, sleep) = if diff >= TRANSITION_SPEED_MS {
                    (
                        // TODO unit test this, so we can't overshoot or undershoot using step
                        dir * ((diff as f64 / TRANSITION_SPEED_MS as f64).floor().max(1.0) as i64),
                        1,
                    )
                } else {
                    (dir, SLEEP_MS.min(TRANSITION_SPEED_MS / diff))
                };

                self.target = Some(Target {
                    desired,
                    step,
                    sleep,
                });
            }
        };
    }

    fn transition(&mut self) -> Result<(), Box<dyn Error>> {
        if let Some(target) = &self.target {
            if target.reached(self.current) {
                self.target = None;
            } else {
                let new_value = (self.current as i64 + target.step).max(0) as u64;
                self.current = self.brightness.set(new_value)?;
                thread::sleep(Duration::from_millis(target.sleep));
            }
        }

        Ok(())
    }
}

fn abs_diff(x: u64, y: u64) -> u64 {
    if x > y {
        x - y
    } else {
        y - x
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brightness::MockBrightness;
    use std::sync::mpsc;

    // Intentionally not in main code to prevent confusing fields by accident
    fn target(desired: u64, step: i64, sleep: u64) -> Target {
        Target {
            desired,
            step,
            sleep,
        }
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
        controller.step()?;

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
        controller.target = Some(target(77, 1, 1));

        // when we execute the next step...
        controller.step()?;

        // we notice a change in brightness made by user and that takes priority
        assert_eq!(42, controller.current);
        assert_eq!(42, user_rx.try_recv()?);
        assert_eq!(true, controller.target.is_none());

        Ok(())
    }

    #[test]
    fn test_target_reached() {
        assert_eq!(false, target(10, 1, 1).reached(9));
        assert_eq!(true, target(10, 1, 1).reached(10));
        assert_eq!(true, target(10, 1, 1).reached(11));

        assert_eq!(true, target(10, -1, 1).reached(9));
        assert_eq!(true, target(10, -1, 1).reached(10));
        assert_eq!(false, target(10, -1, 1).reached(11));
    }
}
