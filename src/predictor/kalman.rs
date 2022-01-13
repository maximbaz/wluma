pub struct Kalman {
    q: f64,
    r: f64,
    covariance: f64,
    value: Option<f64>,
    steps: u64,
}

impl Kalman {
    pub fn new(q: f64, r: f64, covariance: f64) -> Kalman {
        Kalman {
            q,
            r,
            covariance,
            value: None,
            steps: 0,
        }
    }

    pub fn process(&mut self, next: u64) -> u64 {
        self.steps += 1;

        match self.value {
            None => self.value = Some(next as f64),
            Some(prev) => {
                let p0 = self.covariance + self.q;
                let k = p0 / (p0 + self.r);
                self.value = Some(prev + k * (next as f64 - prev));
                self.covariance = (1.0 - k) * p0;
            }
        }

        self.value.unwrap().round() as u64
    }
}
