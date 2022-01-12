use ddc_hi::{Ddc, Display};
use std::cell::RefCell;
use std::error::Error;

pub struct DdcUtil {
    display: RefCell<Display>,
    max_brightness: u64,
}

impl DdcUtil {
    pub fn new(serial_number: &str) -> Result<Self, Box<dyn Error>> {
        let mut display = find_display_by_sn(serial_number).ok_or("Unable to find display")?;
        let max_brightness = get_max_brightness(&mut display)?;

        Ok(Self {
            display: RefCell::new(display),
            max_brightness,
        })
    }
}

impl super::Brightness for DdcUtil {
    fn get(&self) -> Result<u64, Box<dyn Error>> {
        Ok(self
            .display
            .borrow_mut()
            .handle
            .get_vcp_feature(0x10)?
            .value() as u64)
    }

    fn set(&self, value: u64) -> Result<u64, Box<dyn Error>> {
        let value = value.max(1).min(self.max_brightness);
        self.display
            .borrow_mut()
            .handle
            .set_vcp_feature(0x10, value as u16)?;
        Ok(value)
    }
}

fn get_max_brightness(display: &mut Display) -> Result<u64, Box<dyn Error>> {
    Ok(display.handle.get_vcp_feature(0x10)?.maximum() as u64)
}

fn find_display_by_sn(serial_number: &str) -> Option<Display> {
    ddc_hi::Display::enumerate()
        .into_iter()
        .find_map(|mut display| {
            display
                .info
                .serial_number
                .as_ref()
                .map(|v| v == serial_number)
                .and_then(|_| display.update_capabilities().ok())
                .map(|_| display)
        })
}
