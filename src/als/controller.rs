use smol::channel::Sender;
use smol::Timer;

use super::Als;
use std::time::Duration;

const WAITING_SLEEP_MS: u64 = 100;

pub struct Controller {
    als: Als,
    value_txs: Vec<Sender<String>>,
}

impl Controller {
    pub fn new(als: Als, value_txs: Vec<Sender<String>>) -> Self {
        Self { als, value_txs }
    }

    pub async fn run(&mut self) {
        loop {
            self.step().await;
        }
    }

    async fn step(&mut self) {
        match self.als.get().await {
            Ok(value) => {
                futures_util::future::try_join_all(
                    self.value_txs.iter().map(|chan| chan.send(value.clone())),
                )
                .await
                .expect("Unable to send new ALS value, channel is dead");
            }
            Err(err) => log::error!("Unable to get ALS value: {:?}", err),
        };

        Timer::after(Duration::from_millis(WAITING_SLEEP_MS)).await;
    }
}
