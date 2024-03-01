use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::process::{Command, Output};
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::Duration;

const DEFAULT_LUX: u64 = 100;
const WAITING_SLEEP_MS: u64 = 2000;

pub struct Cmd {
    cmd_tx: Sender<u64>,
    command: String,
}

impl Cmd {
    pub fn new(cmd_tx: Sender<u64>, command: String) -> Self {
        Self { cmd_tx, command }
    }

    pub fn run(&mut self) {
        loop {
            self.step();
        }
    }

    fn step(&mut self) {
        if let Ok(lux) = self.output() {
            self.cmd_tx
                .send(lux)
                .expect("Unable to send new webcam lux value, channel is dead");
        };

        thread::sleep(Duration::from_millis(WAITING_SLEEP_MS));
    }

    fn output(&mut self) -> Result<u64, Box<dyn Error>> {
        let Output { status, stdout, .. } =
            Command::new("sh").arg("-c").arg(&self.command).output()?;

        if !status.success() {
            let cmd = &self.command;
            log::warn!("Command {cmd:?} failed: {status}");
            Err(format!("Command {cmd:?} failed: {status}"))?;
        }

        let lux = String::from_utf8(stdout)?.parse()?;

        Ok(lux)
    }
}

pub struct Als {
    cmd_rx: Receiver<u64>,
    thresholds: HashMap<u64, String>,
    lux: RefCell<u64>,
}

impl Als {
    pub fn new(cmd_rx: Receiver<u64>, thresholds: HashMap<u64, String>) -> Self {
        Self {
            cmd_rx,
            thresholds,
            lux: RefCell::new(DEFAULT_LUX),
        }
    }

    fn get_raw(&self) -> Result<u64, Box<dyn Error>> {
        let new_value = self.cmd_rx.try_iter().last().unwrap_or(*self.lux.borrow());
        *self.lux.borrow_mut() = new_value;
        Ok(new_value)
    }
}

impl super::Als for Als {
    fn get(&self) -> Result<String, Box<dyn Error>> {
        let raw = self.get_raw()?;
        let profile = super::find_profile(raw, &self.thresholds);

        log::trace!("ALS (cmd): {} ({})", profile, raw);
        Ok(profile)
    }
}
