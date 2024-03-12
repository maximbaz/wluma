use itertools::Itertools;
use std::collections::HashMap;
use std::error::Error;

pub mod cmd;
pub mod controller;
pub mod iio;
pub mod none;
pub mod time;
pub mod webcam;

pub trait Als {
    fn get(&self) -> Result<String, Box<dyn Error>>;
}

fn find_profile(raw: u64, thresholds: &HashMap<u64, String>) -> String {
    thresholds
        .iter()
        .sorted_by_key(|(lux, _)| *lux)
        .rev()
        .find_or_last(|(lux, _)| raw >= **lux)
        .map(|(_, profile)| profile.to_string())
        .unwrap_or_else(|| panic!("Unable to find ALS profile for value '{}'", raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_profile_base_cases() {
        let thresholds = vec![(0, "dark"), (10, "dim"), (20, "bright")]
            .into_iter()
            .map(|(lux, profile)| (lux, profile.to_string()))
            .collect();

        assert_eq!("dark", find_profile(0, &thresholds));
        assert_eq!("dark", find_profile(2, &thresholds));
        assert_eq!("dim", find_profile(10, &thresholds));
        assert_eq!("dim", find_profile(19, &thresholds));
        assert_eq!("bright", find_profile(20, &thresholds));
        assert_eq!("bright", find_profile(200, &thresholds));
    }

    #[test]
    fn test_find_profile_fallback_first() {
        let thresholds = vec![(5, "dark"), (10, "dim"), (20, "bright")]
            .into_iter()
            .map(|(lux, profile)| (lux, profile.to_string()))
            .collect();

        assert_eq!("dark", find_profile(0, &thresholds));
        assert_eq!("dark", find_profile(4, &thresholds));
    }

    #[test]
    fn test_find_profile_is_constant_on_thresholds_with_one_value() {
        let thresholds = vec![(5, "dark")]
            .into_iter()
            .map(|(lux, profile)| (lux, profile.to_string()))
            .collect();

        assert_eq!("dark", find_profile(0, &thresholds));
        assert_eq!("dark", find_profile(4, &thresholds));
        assert_eq!("dark", find_profile(5, &thresholds));
        assert_eq!("dark", find_profile(9, &thresholds));
    }

    #[test]
    #[should_panic]
    fn test_find_profile_panics_on_empty_thresholds() {
        find_profile(10, &HashMap::default());
    }
}
