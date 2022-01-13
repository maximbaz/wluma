use super::Als;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

const WAITING_SLEEP_MS: u64 = 100;

pub struct Controller {
    als: Box<dyn Als>,
    value_txs: Vec<Sender<String>>,
}

impl Controller {
    pub fn new(als: Box<dyn Als>, value_txs: Vec<Sender<String>>) -> Self {
        Self { als, value_txs }
    }

    pub fn run(&mut self) {
        loop {
            self.step();
        }
    }

    fn step(&mut self) {
        match self.als.get() {
            Ok(value) => {
                self.value_txs.iter().for_each(|chan| {
                    chan.send(value.clone())
                        .expect("Unable to send new ALS value, channel is dead")
                });
            }
            Err(err) => log::error!("Unable to get ALS value: {:?}", err),
        };

        thread::sleep(Duration::from_millis(WAITING_SLEEP_MS));
    }
}
