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

    #[cfg(test)]
    Mock {
        get: Vec<u64>,
        set: Vec<u64>,
    },
}

impl Brightness {
    pub async fn get(&mut self) -> Result<u64, ErrorBox> {
        match self {
            Brightness::DdcUtil(b) => b.get().await,
            Brightness::Backlight(b) => b.get().await,

            #[cfg(test)]
            Brightness::Mock { get, .. } => Ok(get.remove(0)),
        }
    }

    pub async fn set(&mut self, value: u64) -> Result<u64, ErrorBox> {
        match self {
            Brightness::DdcUtil(b) => b.set(value).await,
            Brightness::Backlight(b) => b.set(value).await,

            #[cfg(test)]
            Brightness::Mock { set, .. } => {
                assert_eq!(set.remove(0), value);
                Ok(value)
            }
        }
    }
}
