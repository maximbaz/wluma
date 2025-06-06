mod backlight;
mod controller;
mod ddcutil;

pub use backlight::Backlight;
pub use controller::Controller;
pub use ddcutil::DdcUtil;

use crate::ErrorBox;

#[allow(clippy::large_enum_variant)]
pub enum Brightness {
    DdcUtil(ddcutil::DdcUtil),
    Backlight(backlight::Backlight),
}

impl Brightness {
    pub async fn get(&mut self) -> Result<u64, ErrorBox> {
        match self {
            Brightness::DdcUtil(b) => b.get().await,
            Brightness::Backlight(b) => b.get().await,
        }
    }

    pub async fn set(&mut self, value: u64) -> Result<u64, ErrorBox> {
        match self {
            Brightness::DdcUtil(b) => b.set(value).await,
            Brightness::Backlight(b) => b.set(value).await,
        }
    }
}
