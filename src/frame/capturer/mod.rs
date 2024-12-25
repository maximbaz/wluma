use crate::predictor::data::Entry;
use itertools::Itertools;

pub mod none;
pub mod wayland;

pub trait Adjustable {
    fn adjust(&mut self, luma: u8);

    fn interpolate(&self, entries: &[Entry], lux: &str, luma: u8) -> Option<u64> {
        let points = entries
            .iter()
            .filter(|e| e.lux == lux)
            .map(|entry| {
                let distance = (luma as f64 - entry.luma as f64).abs();
                (entry.brightness as f64, distance)
            })
            .collect_vec();

        if points.is_empty() {
            return None;
        }

        let points = points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let other_distances: f64 = points[0..i]
                    .iter()
                    .chain(&points[i + 1..])
                    .map(|p| p.1)
                    .product();
                (p.0, p.1, other_distances)
            })
            .collect_vec();

        let distance_denominator: f64 = points
            .iter()
            .map(|p| p.1)
            .combinations(points.len() - 1)
            .map(|c| c.iter().product::<f64>())
            .sum();

        let prediction = points
            .iter()
            .map(|p| p.0 * p.2 / distance_denominator)
            .sum::<f64>() as u64;

        Some(prediction)
    }
}

pub trait Capturer {
    fn run(&mut self, output_name: &str, controller: Box<dyn Adjustable>);
}
