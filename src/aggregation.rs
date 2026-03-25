use std::collections::VecDeque;

use crate::settings::AggregationMode;

/// Apply a sliding-window aggregation over a ring buffer.
/// Returns a Vec of the same length as the input.
/// Each output[i] = aggregate of ring[max(0, i-window+1)..=i].
pub fn aggregate_ring(ring: &VecDeque<u64>, mode: AggregationMode, window: usize) -> Vec<u64> {
    let len = ring.len();
    if len == 0 || window <= 1 {
        return ring.iter().copied().collect();
    }

    match mode {
        AggregationMode::Last => ring.iter().copied().collect(),
        AggregationMode::Average => {
            let mut out = Vec::with_capacity(len);
            let mut sum: u64 = 0;
            for i in 0..len {
                sum += ring[i];
                if i >= window {
                    sum -= ring[i - window];
                }
                let count = (i + 1).min(window) as u64;
                out.push(sum / count);
            }
            out
        }
        AggregationMode::Max => {
            let mut out = Vec::with_capacity(len);
            for i in 0..len {
                let start = i.saturating_sub(window - 1);
                let mut max_val = 0u64;
                for val in ring.range(start..=i) {
                    max_val = max_val.max(*val);
                }
                out.push(max_val);
            }
            out
        }
    }
}

/// Return the single aggregated value for the most recent `window` entries.
pub fn aggregate_latest(ring: &VecDeque<u64>, mode: AggregationMode, window: usize) -> u64 {
    if ring.is_empty() {
        return 0;
    }
    let len = ring.len();
    let start = len.saturating_sub(window);

    match mode {
        AggregationMode::Last => *ring.back().unwrap_or(&0),
        AggregationMode::Average => {
            let count = len - start;
            if count == 0 {
                0
            } else {
                let sum: u64 = (start..len).map(|i| ring[i]).sum();
                sum / count as u64
            }
        }
        AggregationMode::Max => (start..len).map(|i| ring[i]).max().unwrap_or(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring(data: &[u64]) -> VecDeque<u64> {
        data.iter().copied().collect()
    }

    #[test]
    fn last_mode_is_identity() {
        let r = ring(&[1, 2, 3, 4, 5]);
        assert_eq!(aggregate_ring(&r, AggregationMode::Last, 3), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn window_one_is_identity() {
        let r = ring(&[10, 20, 30]);
        assert_eq!(aggregate_ring(&r, AggregationMode::Average, 1), vec![10, 20, 30]);
        assert_eq!(aggregate_ring(&r, AggregationMode::Max, 1), vec![10, 20, 30]);
    }

    #[test]
    fn average_sliding_window() {
        let r = ring(&[0, 0, 4, 0, 0]);
        let result = aggregate_ring(&r, AggregationMode::Average, 3);
        // [0, 0, (0+0+4)/3=1, (0+4+0)/3=1, (4+0+0)/3=1]
        assert_eq!(result, vec![0, 0, 1, 1, 1]);
    }

    #[test]
    fn max_sliding_window() {
        let r = ring(&[1, 5, 2, 3, 1]);
        let result = aggregate_ring(&r, AggregationMode::Max, 3);
        // [1, 5, 5, 5, 3]
        assert_eq!(result, vec![1, 5, 5, 5, 3]);
    }

    #[test]
    fn aggregate_latest_avg() {
        let r = ring(&[10, 20, 30, 40]);
        assert_eq!(aggregate_latest(&r, AggregationMode::Average, 2), 35); // (30+40)/2
    }

    #[test]
    fn aggregate_latest_max() {
        let r = ring(&[10, 50, 30, 40]);
        assert_eq!(aggregate_latest(&r, AggregationMode::Max, 3), 50); // max(50,30,40)
    }

    #[test]
    fn aggregate_latest_last() {
        let r = ring(&[10, 20, 30]);
        assert_eq!(aggregate_latest(&r, AggregationMode::Last, 5), 30);
    }

    #[test]
    fn empty_ring() {
        let r: VecDeque<u64> = VecDeque::new();
        assert_eq!(aggregate_ring(&r, AggregationMode::Average, 3), Vec::<u64>::new());
        assert_eq!(aggregate_latest(&r, AggregationMode::Average, 3), 0);
    }

    #[test]
    fn window_larger_than_ring() {
        let r = ring(&[10, 20]);
        assert_eq!(aggregate_latest(&r, AggregationMode::Average, 10), 15); // (10+20)/2
        assert_eq!(aggregate_latest(&r, AggregationMode::Max, 10), 20);
    }
}
