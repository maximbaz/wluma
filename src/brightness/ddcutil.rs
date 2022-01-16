use ddc_hi::{Ddc, Display, FeatureCode};
use std::cell::RefCell;
use std::error::Error;

const DDC_BRIGHTNESS_FEATURE: FeatureCode = 0x10;

pub struct DdcUtil {
    display: RefCell<Display>,
    max_brightness: u64,
}

impl DdcUtil {
    pub fn new(name: &str) -> Result<Self, Box<dyn Error>> {
        let mut display = find_display_by_name(name).ok_or("Unable to find display")?;
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
            .get_vcp_feature(DDC_BRIGHTNESS_FEATURE)?
            .value() as u64)
    }

    fn set(&self, value: u64) -> Result<u64, Box<dyn Error>> {
        let value = value.max(1).min(self.max_brightness);
        self.display
            .borrow_mut()
            .handle
            .set_vcp_feature(DDC_BRIGHTNESS_FEATURE, value as u16)?;
        Ok(value)
    }
}

fn get_max_brightness(display: &mut Display) -> Result<u64, Box<dyn Error>> {
    Ok(display
        .handle
        .get_vcp_feature(DDC_BRIGHTNESS_FEATURE)?
        .maximum() as u64)
}

fn find_display_by_name(name: &str) -> Option<Display> {
    let model = |display: &Display| display.info.model_name.clone();
    let serial = |display: &Display| display.info.serial_number.clone();

    ddc_hi::Display::enumerate()
        .into_iter()
        .find_map(|mut display| {
            log::debug!("display found: {:?}", display.info);
            model(&display)
                .and_then(|model| serial(&display).map(|serial| format!("{} {}", model, serial)))
                .and_then(|merged| merged.contains(name).then(|| ()))
                .and_then(|_| display.update_capabilities().ok())
                .map(|_| display)
        })
}
