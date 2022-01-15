use ddc_hi::{Ddc, Display, FeatureCode};
use itertools::Itertools;
use std::cell::RefCell;
use std::error::Error;

const DDC_BRIGHTNESS_FEATURE: FeatureCode = 0x10;

pub struct DdcUtil {
    display: RefCell<Display>,
    min_brightness: u64,
    max_brightness: u64,
}

impl DdcUtil {
    pub fn new(name: &str) -> Result<Self, Box<dyn Error>> {
        let mut display = find_display_by_name(name).ok_or("Unable to find display")?;
        let max_brightness = get_max_brightness(&mut display)?;

        Ok(Self {
            display: RefCell::new(display),
            min_brightness,
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
        let value = value.max(self.min_brightness).min(self.max_brightness);
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
    let displays = ddc_hi::Display::enumerate()
        .into_iter()
        .filter_map(|mut display| {
            display.update_capabilities().ok().map(|_| {
                let empty = "".to_string();
                let merged = format!(
                    "{} {}",
                    display.info.model_name.as_ref().unwrap_or(&empty),
                    display.info.serial_number.as_ref().unwrap_or(&empty)
                );
                (merged, display)
            })
        })
        .collect_vec();

    log::debug!(
        "ddcutil: Discovered displays: {:?}",
        displays.iter().map(|(name, _)| name).collect_vec()
    );

    displays.into_iter().find_map(|(merged, display)| {
        merged
            .contains(name)
            .then(|| {
                log::debug!("ddcutil: Using display '{}' for config '{}'", merged, name);
            })
            .map(|_| display)
    })
}
