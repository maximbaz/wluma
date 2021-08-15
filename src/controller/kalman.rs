pub struct Kalman {
    steps: u64,
    q: f64,
    r: f64,
    value: Option<f64>,
    covariance: f64,
}

impl Kalman {
    pub fn new(q: f64, r: f64, covariance: f64) -> Kalman {
        Kalman {
            steps: 0,
            q,
            r,
            value: None,
            covariance,
        }
    }
    pub fn process(&mut self, input: f64) -> f64 {
        self.steps += 1;
        match self.value {
            None => {
                self.value = Some(input);
                input
            }
            Some(x0) => {
                let p0 = self.covariance + self.q;
                let k = p0 / (p0 + self.r);
                let x1 = x0 + k * (input - x0);
                let cov = (1.0 - k) * p0;
                self.value = Some(x1);
                self.covariance = cov;
                x1
            }
        }
    }
    pub fn initialized(&self) -> bool {
        self.steps > 10
    }
}
