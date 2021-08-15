use crate::als::Als;
use crate::controller::data::{Data, Entry};
use crate::controller::kalman::Kalman;
use crate::Backlight;
use itertools::Itertools;
use nalgebra as na;
use std::cmp::Ordering::Equal;
use std::cmp::{max, min};
use std::error::Error;
use std::ops::Sub;
use std::thread;
use std::time::Duration;

const TRANSITION_SPEED: u64 = 200;
const PENDING_COOLDOWN_RESET: u8 = 15;

pub struct Controller {
    brightness: Backlight,
    als: Box<dyn Als>,
    kalman: Kalman,
    last_brightness: u64,
    pending_cooldown: u8,
    pending: Option<Entry>,
    data: Data,
    lux_max_seen: f64,
}

impl Controller {
    pub fn new(brightness: Backlight, als: Box<dyn Als>) -> Self {
        Self {
            brightness,
            als,
            kalman: Kalman::new(1., 20., 10.),
            last_brightness: 0,
            pending_cooldown: 0,
            pending: None,
            data: Data::load().unwrap_or_default(),
            lux_max_seen: 1.0,
        }
    }

    pub fn adjust(&mut self, luma: Option<u8>) -> Result<(), Box<dyn Error>> {
        let lux = self.als.get()?;
        let lux = self.kalman.process(lux as f64).round() as u64; // TODO make Kalman::<u64>
        let brightness = self.brightness.get().unwrap();
        if !self.kalman.initialized() {
            self.last_brightness = brightness;
            return Ok(());
        }

        let brightness_changed = self.last_brightness != brightness;
        let no_data_points = self.data.entries.is_empty() && self.pending_cooldown == 0;

        self.last_brightness = if brightness_changed || no_data_points {
            self.init_user_changed_brightness(lux, luma, brightness)
        } else if self.pending_cooldown > 1 {
            self.cooldown_user_changed_brightness(brightness)
        } else if self.pending_cooldown == 1 {
            self.commit_user_changed_brightness(brightness)
        } else {
            self.predict_set_brightness(lux, luma, brightness)
        };

        Ok(())
    }

    fn init_user_changed_brightness(&mut self, lux: u64, luma: Option<u8>, brightness: u64) -> u64 {
        if self.pending_cooldown == 0 {
            self.pending = Some(Entry {
                lux,
                luma: luma.unwrap(),
                brightness,
            });
        } else {
            self.pending.as_mut().unwrap().brightness = brightness;
        }
        self.pending_cooldown = PENDING_COOLDOWN_RESET;
        brightness
    }

    fn cooldown_user_changed_brightness(&mut self, brightness: u64) -> u64 {
        self.pending_cooldown -= 1;
        brightness
    }

    fn commit_user_changed_brightness(&mut self, brightness: u64) -> u64 {
        self.pending_cooldown = 0;

        let pending = self.pending.take().unwrap();
        self.lux_max_seen = self.lux_max_seen.max(pending.lux as f64);

        self.data.entries.retain(|elem| {
            !((elem.lux == pending.lux && elem.luma == pending.luma)
                || (elem.lux > pending.lux && elem.luma == pending.luma)
                || (elem.lux < pending.lux
                    && elem.luma >= pending.luma
                    && elem.brightness > pending.brightness)
                || (elem.lux == pending.lux
                    && elem.luma < pending.luma
                    && elem.brightness < pending.brightness)
                || (elem.lux > pending.lux
                    && elem.luma <= pending.luma
                    && elem.brightness < pending.brightness)
                || (elem.lux == pending.lux
                    && elem.luma > pending.luma
                    && elem.brightness > pending.brightness))
        });

        self.data.entries.push(pending); // TODO investigate derive Copy

        self.data.save().expect("Unable to save data to a file");

        brightness
    }

    fn predict_set_brightness(&mut self, lux: u64, luma: Option<u8>, brightness: u64) -> u64 {
        let lux_f = lux as f64;
        let luma_f = luma.unwrap() as f64;

        let lux_capped = self.lux_max_seen.min(lux_f);

        let nearest = self
            .data
            .entries
            .iter()
            .map(|elem| {
                let dist_lux = (lux_capped - elem.lux as f64) * 100.0 / self.lux_max_seen;
                let dist_luma = luma_f - elem.luma as f64;
                let dist = (dist_lux.powf(2.0) + dist_luma.powf(2.0)).sqrt();
                let point = (elem.lux as f64, elem.luma as f64, elem.brightness as f64);

                (point, dist)
            })
            .sorted_by(|(_, a), (_, b)| PartialOrd::partial_cmp(a, b).unwrap_or(Equal))
            .take(3)
            .map(|(elem, _)| elem)
            .collect_vec();

        let mut target_value = nearest[0].2 as u64;

        if nearest.len() == 3 {
            let plane_vec1 = na::Vector3::new(
                nearest[0].0 - nearest[1].0,
                nearest[0].1 - nearest[1].1,
                nearest[0].2 - nearest[1].2,
            );
            let plane_vec2 = na::Vector3::new(
                nearest[0].0 - nearest[2].0,
                nearest[0].1 - nearest[2].1,
                nearest[0].2 - nearest[2].2,
            );

            let plane_normal = plane_vec1.cross(&plane_vec2).normalize();

            let line_point1 = na::Vector3::new(lux_f, luma_f, 0.0);
            let line_direction = na::Vector3::new(0.0, 0.0, -100.0).normalize();

            let plane_line_dot = plane_normal.dot(&line_direction);
            if plane_line_dot > 0.0001 {
                let plane_point = na::Vector3::new(nearest[0].0, nearest[0].1, nearest[0].2);
                let line_plane_diff = line_point1.sub(&plane_point);
                let scale = plane_normal.dot(&line_plane_diff) / plane_line_dot;

                let line_direction_scaled = line_direction.scale(scale);
                let intersection = line_point1.sub(line_direction_scaled);

                target_value = 1.0_f64.max(100.0_f64.min(intersection.z.round())) as u64;
            }
        }

        if brightness != target_value {
            self.change_brightness(brightness, target_value);
        }
        target_value
    }

    fn change_brightness(&self, mut last_value: u64, value: u64) {
        let diff = max(value, last_value) - min(value, last_value);
        let dir = if last_value < value { 1 } else { -1 };
        let (step, sleep) = if diff >= TRANSITION_SPEED {
            (diff / TRANSITION_SPEED, 1)
        } else {
            (1, TRANSITION_SPEED / diff)
        };

        while dir > 0 && last_value < value || dir < 0 && last_value > value {
            let new_value = ((last_value as i64) + (step as i64) * dir) as u64;
            self.brightness.set(new_value).unwrap();
            last_value = new_value;
            thread::sleep(Duration::from_millis(sleep));
        }
    }
}
