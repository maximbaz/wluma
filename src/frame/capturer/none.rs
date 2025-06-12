use std::time::Duration;

use smol::Timer;

#[derive(Default)]
pub struct Capturer {}

impl Capturer {
    pub async fn run(&mut self, _output_name: &str, mut controller: crate::predictor::Controller) {
        loop {
            controller.adjust(0).await;
            Timer::after(Duration::from_millis(200)).await;
        }
    }
}
