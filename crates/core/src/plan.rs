//! Segmentation plan for a multi-connection download.
//!
//! The plan owns a set of byte ranges that together cover the whole file.
//! Workers "acquire" a bounded batch from the largest unfinished, not-currently
//! in-flight range, stream it, and then commit the batch. Ranges are split in
//! half whenever parallelism droops, which keeps every connection busy until
//! the very end (the same trick IDM-style accelerators use).

use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange {
    pub start: u64,
    pub end: u64,
    /// Next byte that still has to be fetched.
    pub pos: u64,
}

impl ByteRange {
    fn remaining(&self) -> u64 {
        self.end.saturating_sub(self.pos)
    }

    fn is_complete(&self) -> bool {
        self.pos >= self.end
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BatchJob {
    pub index: usize,
    pub range_start: u64,
    pub range_end: u64,
    /// Absolute first byte of this batch.
    pub start: u64,
    /// Absolute end (exclusive) of this batch.
    pub end: u64,
}

impl BatchJob {
    pub fn len(&self) -> u64 {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Debug, Clone)]
pub struct RangePlan {
    ranges: Vec<ByteRange>,
    in_flight: HashSet<usize>,
    target_workers: u32,
    batch_size: u64,
    min_split_size: u64,
}

impl RangePlan {
    /// Split `total` bytes into an initial set of ranges, one per worker.
    pub fn new(total: u64, workers: u32, batch_size: u64, min_split_size: u64) -> Self {
        let workers = workers.max(1) as u64;
        let batch_size = batch_size.max(1);
        let min_split_size = min_split_size.max(batch_size);

        let piece_floor = min_split_size.max(1);
        let max_pieces = (total / piece_floor).max(1);
        let count = workers.min(max_pieces).max(1) as usize;

        let mut ranges = Vec::with_capacity(count);
        let base = total / count as u64;
        let remainder = total % count as u64;

        for index in 0..count {
            let start = index as u64 * base + (index as u64).min(remainder);
            let length = base + u64::from((index as u64) < remainder);
            ranges.push(ByteRange {
                start,
                end: start + length,
                pos: start,
            });
        }

        Self {
            ranges,
            in_flight: HashSet::new(),
            target_workers: workers as u32,
            batch_size,
            min_split_size,
        }
    }

    /// Rebuild a plan from persisted `(start, end, pos)` rows.
    pub fn from_rows(
        rows: Vec<(u64, u64, u64)>,
        workers: u32,
        batch_size: u64,
        min_split_size: u64,
    ) -> Self {
        let mut ranges: Vec<ByteRange> = rows
            .into_iter()
            .filter(|(start, end, _)| start < end)
            .map(|(start, end, pos)| ByteRange {
                start,
                end,
                pos: pos.clamp(start, end),
            })
            .collect();
        ranges.sort_by_key(|range| range.start);

        if ranges.is_empty() {
            return Self::new(0, workers, batch_size, min_split_size);
        }

        Self {
            ranges,
            in_flight: HashSet::new(),
            target_workers: workers.max(1),
            batch_size: batch_size.max(1),
            min_split_size: min_split_size.max(1),
        }
    }

    pub fn snapshot(&self) -> Vec<(u64, u64, u64)> {
        self.ranges
            .iter()
            .map(|range| (range.start, range.end, range.pos))
            .collect()
    }

    pub fn total_size(&self) -> u64 {
        self.ranges
            .iter()
            .map(|range| range.end.saturating_sub(range.start))
            .sum()
    }

    /// Bytes that have been committed to the partial file.
    pub fn received(&self) -> u64 {
        self.ranges
            .iter()
            .map(|range| range.pos.saturating_sub(range.start))
            .sum()
    }

    pub fn remaining(&self) -> u64 {
        self.ranges.iter().map(ByteRange::remaining).sum()
    }

    pub fn is_complete(&self) -> bool {
        self.in_flight.is_empty() && self.remaining() == 0
    }

    pub fn has_work(&self) -> bool {
        self.remaining() > 0
    }

    /// Split the largest idle range to increase parallelism. Returns `true` if
    /// a split happened.
    fn maybe_split(&mut self) -> bool {
        let incomplete = self
            .ranges
            .iter()
            .filter(|range| !range.is_complete())
            .count();
        if incomplete as u64 >= self.target_workers as u64 {
            return false;
        }

        let candidate = self
            .ranges
            .iter()
            .enumerate()
            .filter(|(index, range)| !self.in_flight.contains(index) && !range.is_complete())
            .max_by_key(|(_, range)| range.remaining())
            .map(|(index, range)| (index, *range));

        let Some((index, range)) = candidate else {
            return false;
        };
        if range.remaining() < self.min_split_size.saturating_mul(2) {
            return false;
        }

        let midpoint = range.pos + range.remaining() / 2;
        if midpoint <= range.pos || midpoint >= range.end {
            return false;
        }

        self.ranges[index].end = midpoint;
        self.ranges.push(ByteRange {
            start: midpoint,
            end: range.end,
            pos: midpoint,
        });
        true
    }

    /// IDM-style dynamic segmentation: When a worker is free and all existing
    /// ranges are in-flight, split the unreserved tail of the largest in-flight range.
    /// This prevents tail-latency stragglers and ensures all connections remain 100% busy.
    fn maybe_split_in_flight(&mut self) -> bool {
        let incomplete = self
            .ranges
            .iter()
            .filter(|range| !range.is_complete())
            .count();
        if incomplete as u64 >= self.target_workers as u64 {
            return false;
        }

        let candidate = self
            .ranges
            .iter()
            .enumerate()
            .filter(|(index, range)| self.in_flight.contains(index) && !range.is_complete())
            .filter_map(|(index, range)| {
                let reserved_end = range.pos.saturating_add(self.batch_size).min(range.end);
                let unreserved = range.end.saturating_sub(reserved_end);
                if unreserved >= self.min_split_size.saturating_mul(2) {
                    Some((index, *range, reserved_end, unreserved))
                } else {
                    None
                }
            })
            .max_by_key(|(_, _, _, unreserved)| *unreserved);

        let Some((index, range, reserved_end, unreserved)) = candidate else {
            return false;
        };

        let midpoint = reserved_end + unreserved / 2;
        if midpoint <= reserved_end || midpoint >= range.end {
            return false;
        }

        self.ranges[index].end = midpoint;
        self.ranges.push(ByteRange {
            start: midpoint,
            end: range.end,
            pos: midpoint,
        });
        true
    }

    /// Reserve a batch from the largest idle unfinished range.
    pub fn acquire(&mut self) -> Option<BatchJob> {
        if !self.has_work() {
            return None;
        }

        self.maybe_split();

        let mut candidate = self
            .ranges
            .iter()
            .enumerate()
            .filter(|(index, range)| !self.in_flight.contains(index) && !range.is_complete())
            .max_by_key(|(_, range)| range.remaining())
            .map(|(index, range)| (index, *range));

        if candidate.is_none() && self.maybe_split_in_flight() {
            candidate = self
                .ranges
                .iter()
                .enumerate()
                .filter(|(index, range)| !self.in_flight.contains(index) && !range.is_complete())
                .max_by_key(|(_, range)| range.remaining())
                .map(|(index, range)| (index, *range));
        }

        let (index, range) = candidate?;
        let batch_end = range.pos.saturating_add(self.batch_size).min(range.end);
        self.in_flight.insert(index);

        Some(BatchJob {
            index,
            range_start: range.start,
            range_end: range.end,
            start: range.pos,
            end: batch_end,
        })
    }

    /// Mark a batch as finished. When `wrote_all` is false the bytes are left
    /// uncommitted so they are re-downloaded on resume.
    pub fn commit(&mut self, job: BatchJob, wrote_all: bool) {
        self.in_flight.remove(&job.index);
        if let Some(range) = self.ranges.get_mut(job.index) {
            if wrote_all && range.start == job.range_start {
                range.pos = job.end.max(range.pos);
            }
        }
    }

    /// Return every reservation to the pool (used when a session aborts).
    pub fn release_all(&mut self) {
        self.in_flight.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn drain(plan: &mut RangePlan) -> Vec<(u64, u64)> {
        let mut committed = Vec::new();
        while plan.has_work() {
            let Some(job) = plan.acquire() else {
                break;
            };
            plan.commit(job, true);
            committed.push((job.start, job.end));
        }
        committed
    }

    fn assert_exact_cover(mut intervals: Vec<(u64, u64)>, total: u64) {
        intervals.sort_unstable();
        let mut next = 0u64;
        for (start, end) in intervals {
            assert_eq!(start, next, "gap or overlap in committed batches");
            assert!(end > start);
            next = end;
        }
        assert_eq!(next, total);
    }

    #[test]
    fn creates_one_range_per_worker_and_covers_file() {
        let plan = RangePlan::new(10 << 20, 4, 1 << 20, 2 << 20);
        assert_eq!(plan.ranges.len(), 4);
        assert_eq!(plan.total_size(), 10 << 20);
        assert_eq!(plan.remaining(), 10 << 20);
        assert_eq!(plan.received(), 0);
    }

    #[test]
    fn splitting_a_partially_downloaded_range_keeps_it_valid() {
        let mut plan = RangePlan::new(200, 2, 10, 10);
        plan.ranges[0].pos = 90;
        plan.ranges[1].pos = 100;

        let job = plan.acquire().expect("work remains");
        assert!(
            job.start >= 90,
            "split must not re-cover downloaded bytes (got {})",
            job.start
        );

        let snapshot = plan.snapshot();
        assert_eq!(
            snapshot
                .iter()
                .map(|(start, end, _)| end.saturating_sub(*start))
                .sum::<u64>(),
            200
        );
        for (start, end, pos) in snapshot {
            assert!(start < end);
            assert!(pos >= start && pos <= end);
        }
    }

    #[test]
    fn small_file_uses_single_range() {
        let plan = RangePlan::new(3 << 20, 8, 1 << 20, 2 << 20);
        assert_eq!(plan.ranges.len(), 1);
    }

    #[test]
    fn dynamic_segmentation_splits_in_flight_range_when_worker_is_available() {
        // 100 MB file, target 4 workers, batch 1 MB, min split 2 MB
        let mut plan = RangePlan::new(100 << 20, 4, 1 << 20, 2 << 20);
        assert_eq!(plan.ranges.len(), 4);

        // Workers 0, 1, 2, 3 acquire batches for each range
        let job0 = plan.acquire().expect("job0");
        let job1 = plan.acquire().expect("job1");
        let job2 = plan.acquire().expect("job2");
        let job3 = plan.acquire().expect("job3");

        // Now all 4 ranges are in-flight!
        assert_eq!(plan.in_flight.len(), 4);

        // Worker 0 finishes and commits its entire piece (simulate completing early)
        plan.commit(job0, true);
        plan.ranges[job0.index].pos = plan.ranges[job0.index].end; // range 0 is now 100% complete

        // Worker 0 asks for more work. All other ranges (1, 2, 3) are in flight!
        // With dynamic segmentation, it splits an in-flight range and gets a new job!
        let new_job = plan
            .acquire()
            .expect("should dynamically acquire work from in-flight range");
        assert_eq!(plan.ranges.len(), 5); // A 5th range was dynamically created!
        assert!(new_job.start < new_job.end);

        // Finish everything
        plan.commit(new_job, true);
        plan.commit(job1, true);
        plan.commit(job2, true);
        plan.commit(job3, true);

        let _intervals = drain(&mut plan);
        assert!(plan.is_complete());
    }

    #[test]
    fn sequential_batches_commit_exactly_once() {
        let total = 9_876_543u64;
        let mut plan = RangePlan::new(total, 4, 256 << 10, 1 << 20);
        let intervals = drain(&mut plan);
        assert!(plan.is_complete());
        assert_eq!(plan.received(), total);
        assert_exact_cover(intervals, total);
    }

    #[test]
    fn failed_batches_are_retried_without_double_counting() {
        let total = 5 << 20;
        let mut plan = RangePlan::new(total, 4, 256 << 10, 1 << 20);
        let mut committed = Vec::new();
        let mut failures = 0;

        while plan.has_work() {
            let Some(job) = plan.acquire() else {
                break;
            };
            if failures < 7 {
                failures += 1;
                plan.commit(job, false);
            } else {
                plan.commit(job, true);
                committed.push((job.start, job.end));
            }
        }

        assert!(plan.is_complete());
        assert_eq!(plan.received(), total);
        assert_exact_cover(committed, total);
    }

    #[test]
    fn resume_from_snapshot_keeps_committed_progress() {
        let total = 8 << 20;
        let mut plan = RangePlan::new(total, 4, 256 << 10, 1 << 20);
        for _ in 0..5 {
            let job = plan.acquire().expect("work available");
            plan.commit(job, true);
        }
        let snapshot = plan.snapshot();
        let received = plan.received();

        let resumed = RangePlan::from_rows(snapshot, 4, 256 << 10, 1 << 20);
        assert_eq!(resumed.received(), received);
        assert_eq!(resumed.total_size(), total);
    }

    proptest! {
        #[test]
        fn random_plans_cover_exactly_once(
            total in 1u64..20_000_000,
            workers in 1u32..32,
            batch in 1u64..2_000_000,
            min_split in 1u64..4_000_000,
        ) {
            let mut plan = RangePlan::new(total, workers, batch, min_split);
            let intervals = drain(&mut plan);
            prop_assert!(plan.is_complete());
            prop_assert_eq!(plan.received(), total);
            assert_exact_cover(intervals, total);
        }
    }
}
