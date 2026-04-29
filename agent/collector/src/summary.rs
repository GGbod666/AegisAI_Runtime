#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueSummary {
    pub samples: u64,
    pub total: u64,
    pub min: Option<u64>,
    pub max: Option<u64>,
    pub mean: Option<u64>,
    pub p50: Option<u64>,
    pub p95: Option<u64>,
    pub p99: Option<u64>,
}

impl ValueSummary {
    pub fn empty() -> Self {
        Self {
            samples: 0,
            total: 0,
            min: None,
            max: None,
            mean: None,
            p50: None,
            p95: None,
            p99: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CounterSummary {
    pub total: u64,
    pub per_second: f64,
}

impl CounterSummary {
    pub fn new(total: u64, window_size_us: u64) -> Self {
        let per_second = if window_size_us == 0 {
            0.0
        } else {
            total as f64 * 1_000_000.0 / window_size_us as f64
        };

        Self { total, per_second }
    }
}

#[derive(Default)]
pub(crate) struct SampleAccumulator {
    values: Vec<u64>,
    total: u128,
}

impl SampleAccumulator {
    pub(crate) fn record(&mut self, value: u64) {
        self.values.push(value);
        self.total += value as u128;
    }

    pub(crate) fn finalize(mut self) -> ValueSummary {
        if self.values.is_empty() {
            return ValueSummary::empty();
        }

        self.values.sort_unstable();

        let len = self.values.len();
        let min = *self.values.first().expect("checked non-empty");
        let max = *self.values.last().expect("checked non-empty");
        let mean = (self.total / len as u128) as u64;
        let p50 = self.values[nearest_rank_index(len, 50, 100)];
        let p95 = self.values[nearest_rank_index(len, 95, 100)];
        let p99 = self.values[nearest_rank_index(len, 99, 100)];

        ValueSummary {
            samples: len as u64,
            total: self.total.min(u64::MAX as u128) as u64,
            min: Some(min),
            max: Some(max),
            mean: Some(mean),
            p50: Some(p50),
            p95: Some(p95),
            p99: Some(p99),
        }
    }
}

fn nearest_rank_index(len: usize, numerator: usize, denominator: usize) -> usize {
    debug_assert!(len > 0);
    debug_assert!(denominator > 0);

    let rank = (len * numerator).div_ceil(denominator);
    rank.saturating_sub(1).min(len - 1)
}

#[cfg(test)]
mod tests {
    use super::SampleAccumulator;

    #[test]
    fn computes_percentiles_with_nearest_rank() {
        let mut acc = SampleAccumulator::default();
        for value in [10, 20, 30, 40, 50] {
            acc.record(value);
        }

        let summary = acc.finalize();

        assert_eq!(summary.p50, Some(30));
        assert_eq!(summary.p95, Some(50));
        assert_eq!(summary.p99, Some(50));
        assert_eq!(summary.mean, Some(30));
    }
}
