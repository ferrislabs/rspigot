const SIZE: usize = 64;
const MASK: i32 = 63;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Sample {
    pub y: f64,
    pub y_rot: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlightHistory {
    samples: [Sample; SIZE],
    head: i32,
}

impl Default for FlightHistory {
    fn default() -> Self {
        Self {
            samples: [Sample::default(); SIZE],
            head: -1,
        }
    }
}

impl FlightHistory {
    pub fn record(&mut self, y: f64, y_rot: f32) {
        let sample = Sample { y, y_rot };
        if self.head < 0 {
            self.samples = [sample; SIZE];
        }
        self.head += 1;
        if self.head == SIZE as i32 {
            self.head = 0;
        }
        self.samples[self.head as usize] = sample;
    }

    pub fn get(&self, delay: i32) -> Sample {
        self.samples[((self.head - delay) & MASK) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_record_fills_every_sample() {
        let mut history = FlightHistory::default();
        history.record(70.0, 90.0);
        assert_eq!(
            history.get(0),
            Sample {
                y: 70.0,
                y_rot: 90.0
            }
        );
        assert_eq!(
            history.get(40),
            Sample {
                y: 70.0,
                y_rot: 90.0
            }
        );
    }

    #[test]
    fn older_samples_are_reached_with_a_delay() {
        let mut history = FlightHistory::default();
        for tick in 0..10 {
            history.record(f64::from(tick), tick as f32);
        }
        assert_eq!(history.get(0).y, 9.0);
        assert_eq!(history.get(5).y, 4.0);
        assert_eq!(history.get(9).y, 0.0);
    }

    #[test]
    fn the_head_wraps_after_sixty_four_samples() {
        let mut history = FlightHistory::default();
        for tick in 0..70 {
            history.record(f64::from(tick), 0.0);
        }
        assert_eq!(history.get(0).y, 69.0);
        assert_eq!(history.get(63).y, 6.0);
        assert_eq!(history.get(64).y, 69.0);
    }

    #[test]
    fn a_delay_before_any_record_reads_the_default_sample() {
        let history = FlightHistory::default();
        assert_eq!(history.get(5), Sample::default());
    }
}
