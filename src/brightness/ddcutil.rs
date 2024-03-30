use ddc_hi::{Ddc, Display, FeatureCode};
use itertools::Itertools;
use lazy_static::lazy_static;
use std::cell::RefCell;
use std::error::Error;
use std::sync::Mutex;

lazy_static! {
    static ref DDC_MUTEX: Mutex<()> = Mutex::new(());
}

const DDC_BRIGHTNESS_FEATURE: FeatureCode = 0x10;

pub struct DdcUtil {
    display: RefCell<Display>,
    min_brightness: u64,
    max_brightness: u64,
}

impl DdcUtil {
    pub fn new(name: &str, min_brightness: u64) -> Result<Self, Box<dyn Error>> {
        let mut display = find_display_by_name(name, true)
            .or_else(|| find_display_by_name(name, false))
            .ok_or("Unable to find display")?;
        let max_brightness = get_max_brightness(&mut display)?;

        Ok(Self {
            display: RefCell::new(display),
            min_brightness,
            max_brightness,
        })
    }
}

impl super::Brightness for DdcUtil {
    fn get(&mut self) -> Result<u64, Box<dyn Error>> {
        let _lock = DDC_MUTEX
            .lock()
            .expect("Unable to acquire exclusive access to DDC API");
        Ok(self
            .display
            .borrow_mut()
            .handle
            .get_vcp_feature(DDC_BRIGHTNESS_FEATURE)?
            .value() as u64)
    }

    fn set(&mut self, value: u64) -> Result<u64, Box<dyn Error>> {
        let _lock = DDC_MUTEX
            .lock()
            .expect("Unable to acquire exclusive access to DDC API");
        let value = value.clamp(self.min_brightness, self.max_brightness);
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

fn find_display_by_name(name: &str, check_caps: bool) -> Option<Display> {
    let displays = ddc_hi::Display::enumerate()
        .into_iter()
        .filter_map(|mut display| {
            let caps = if check_caps {
                display.update_capabilities()
            } else {
                Ok(())
            };
            caps.ok().map(|_| {
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
        "Discovered displays (check_caps={}): {:?}",
        check_caps,
        displays.iter().map(|(name, _)| name).collect_vec()
    );

    displays.into_iter().find_map(|(merged, display)| {
        merged
            .contains(name)
            .then(|| {
                log::debug!(
                    "Using display '{}' for config '{}' (check_caps={})",
                    merged,
                    name,
                    check_caps
                );
            })
            .map(|_| display)
    })
}
