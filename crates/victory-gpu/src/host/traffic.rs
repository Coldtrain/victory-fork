//! Diagnostics: which host copies' bytes cross the bus, and which way: per
//! copy (title address, bytes) what was sent up and what shaders wrote that
//! was fetched, a frame; and how many different 64 KiB pieces of it. Every
//! five seconds, the eight that move most. For placing resources: a copy whose
//! pieces are fetched every frame and never sent up is written by shaders
//! alone, and nobody but shaders may read it.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

/// Of a copy, each way (up, fetched): bytes, and the pieces (64 KiB).
#[derive(Default)]
struct Moved {
    bytes: [u64; 2],
    pieces: [HashSet<u32>; 2],
}

pub(super) const UP: usize = 0;
pub(super) const FETCHED: usize = 1;

static MOVED: Mutex<Option<HashMap<(u64, u64), Moved>>> = Mutex::new(None);

/// Offsets `ranges` of the copy of `span` (title address, bytes) went `way`.
pub(super) fn moved(way: usize, span: (u64, u64), ranges: impl Iterator<Item = (usize, usize)>) {
    let mut all = MOVED.lock().unwrap_or_else(|e| e.into_inner());
    let m = all.get_or_insert_with(Default::default).entry(span).or_default();
    // Thousands of short runs often touch the same 64 KiB piece.
    // Track the whole prior interval so overlaps don't lose an earlier piece.
    let mut previous: Option<(u32, u32)> = None;
    for (from, to) in ranges.filter(|r| r.0 < r.1) {
        m.bytes[way] += (to - from) as u64;
        let (first, end) = ((from >> 16) as u32, ((to - 1) >> 16) as u32);
        match previous {
            Some((old_first, old_end)) if first >= old_first && end <= old_end => {}
            Some((old_first, old_end)) if first >= old_first && first <= old_end => {
                if old_end < end {
                    m.pieces[way].extend(old_end + 1..=end);
                }
            }
            _ => m.pieces[way].extend(first..=end),
        }
        previous = Some((first, end));
    }
}

/// For the log, over `frames` frames.
pub(super) fn line(frames: f64) {
    let all = MOVED.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap_or_default();
    let mut all: Vec<_> = all.into_iter().collect();
    all.sort_by_key(|(_, m)| std::cmp::Reverse(m.bytes[0] + m.bytes[1]));
    let mb = |b: u64| b as f64 / 1e6 / frames.max(1.0);
    let sum = |way: usize| all.iter().map(|(_, m)| m.bytes[way]).sum::<u64>();
    // Copies that only move one way.
    let only = |way: usize| all.iter().filter(|(_, m)| m.bytes[1 - way] == 0).map(|(_, m)| m.bytes[way]).sum::<u64>();
    tracing::info!(
        "landings in title memory looked at by refreshes per frame (VICTORY_LAND_ECHO=0: sent up): {:.1} MB held already and left, \
         {:.1} MB the title's too",
        mb(crate::shadow::ECHO[0].swap(0, std::sync::atomic::Ordering::Relaxed)),
        mb(crate::shadow::ECHO[1].swap(0, std::sync::atomic::Ordering::Relaxed)),
    );
    tracing::info!(
        "bytes of host copies over the bus per frame: {:.1} MB up ({:.1} of copies nothing was fetched from), {:.1} MB fetched \
         ({:.1} of copies nothing was sent up to), {} copies; the most:",
        mb(sum(UP)),
        mb(only(UP)),
        mb(sum(FETCHED)),
        mb(only(FETCHED)),
        all.len()
    );
    for ((start, len), m) in all.into_iter().take(8) {
        let both = m.pieces[0].intersection(&m.pieces[1]).count();
        tracing::info!(
            "  {start:#x}+{len:#x}: {:.2} MB up in {} different pieces, {:.2} MB fetched in {}, {both} of them both ways",
            mb(m.bytes[0]),
            m.pieces[0].len(),
            mb(m.bytes[1]),
            m.pieces[1].len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_and_disjoint_runs_report_every_touched_piece() {
        let span = (0x1000u64, 0x70000u64);
        // A later run may retreat across a piece boundary. Both byte totals
        // and the union of 64 KiB pieces must remain unchanged.
        moved(UP, span, [(0, 0x20010), (0x10000, 0x10004), (0x30000, 0x40001), (0x2ffff, 0x30002)].into_iter());
        let all = MOVED.lock().unwrap_or_else(|e| e.into_inner()).take().unwrap();
        let got = all.get(&span).unwrap();
        assert_eq!(got.bytes[UP], 0x20010 + 4 + 0x10001 + 3);
        assert_eq!(got.pieces[UP], HashSet::from([0, 1, 2, 3, 4]));
    }
}
