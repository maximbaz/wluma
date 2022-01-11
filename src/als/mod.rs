use std::error::Error;

#[cfg(test)]
use mockall::*;

pub mod iio;
pub mod none;
pub mod time;
pub mod webcam;
pub mod controller;

#[cfg_attr(test, automock)]
pub trait Als {
    fn get(&self) -> Result<u64, Box<dyn Error>>;
}

fn smoothen(raw: u64, thresholds: &Vec<u64>) -> u64 {
    thresholds
        .iter()
        .enumerate()
        .find(|(_, &threshold)| raw < threshold)
        .map(|(i, _)| i as u64)
        .unwrap_or(thresholds.len() as u64)
}

fn to_percent(smooth: u64, max: u64) -> Result<u64, String> {
    match max {
        0 => Err("Unable to calculate percentage (division by zero)".to_string()),
        _ => Ok(((smooth as f64) * 100.0 / (max as f64)).ceil() as u64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smoothen() {
        assert_eq!(0, smoothen(123, &vec![]));
        assert_eq!(0, smoothen(23, &vec![100, 200]));
        assert_eq!(1, smoothen(123, &vec![100, 200]));
        assert_eq!(2, smoothen(223, &vec![100, 200]));
    }

    #[test]
    fn test_to_percent() {
        assert_eq!(true, to_percent(10, 0).is_err());
        assert_eq!(Ok(0), to_percent(0, 3));
        assert_eq!(Ok(34), to_percent(1, 3));
        assert_eq!(Ok(67), to_percent(2, 3));
        assert_eq!(Ok(100), to_percent(3, 3));
    }
}
