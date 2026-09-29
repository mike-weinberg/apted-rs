//! Errors of the fallible (`try_`) API.

use std::fmt;

/// Why a distance or mapping could not be computed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TedError {
    /// The trees exceed the algorithm's representable size: node ids are
    /// stored in `f32` (as in the Java implementation), so the combined
    /// node count must stay below 2^24, and the memory estimate must fit in
    /// `usize`.
    TooLarge { size1: usize, size2: usize },
    /// The estimated peak memory is above the limit set with
    /// [`crate::APTED::with_memory_limit`].
    MemoryLimitExceeded { estimated: usize, limit: usize },
    /// The allocator refused the estimated peak memory.
    AllocationFailed { bytes: usize },
    /// The mapping was requested before a distance was computed for the
    /// current pair of trees.
    DistanceNotComputed,
    /// The distance is NaN, so no mapping exists: the cost model returned
    /// NaN for some operation.
    NotANumber,
}

impl fmt::Display for TedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TedError::TooLarge { size1, size2 } => write!(
                f,
                "trees too large for APTED: {size1} + {size2} nodes (the sum must be below 2^24)"
            ),
            TedError::MemoryLimitExceeded { estimated, limit } => write!(
                f,
                "APTED would need about {estimated} bytes, above the limit of {limit} bytes"
            ),
            TedError::AllocationFailed { bytes } => {
                write!(f, "could not allocate {bytes} bytes for APTED")
            }
            TedError::DistanceNotComputed => {
                write!(f, "compute the distance before the mapping")
            }
            TedError::NotANumber => {
                write!(f, "the distance is NaN; the cost model returned NaN")
            }
        }
    }
}

impl std::error::Error for TedError {}

/// Largest combined node count: ids up to `size1 + size2` must be exact
/// in `f32`, which holds for integers up to 2^24.
const MAX_COMBINED_NODES: usize = (1 << 24) - 2;

/// Estimated heap memory, in bytes, that computing the distance between
/// trees of `size1` and `size2` nodes needs in any case: the
/// strategy/distance matrix (n·m) and two forest distance tables (up to
/// (n+1)·(m+1) each), all `f32`. `None` if the trees are too large to
/// process at all (see [`TedError::TooLarge`]).
///
/// Inner strategy paths additionally allocate a square table of up to
/// (max(n, m) + 1)² `f32` when they run; that allocation is checked (against
/// the memory limit and the allocator) at that point.
///
/// Use it to reject oversized input before calling
/// [`crate::APTED::compute_edit_distance`].
pub fn estimated_peak_bytes(size1: usize, size2: usize) -> Option<usize> {
    if size1.checked_add(size2)? > MAX_COMBINED_NODES {
        return None;
    }
    let forest = size1.checked_add(1)?.checked_mul(size2.checked_add(1)?)?;
    let floats = size1
        .checked_mul(size2)?
        .checked_add(forest.checked_mul(2)?)?;
    floats_to_bytes(floats)
}

/// Bytes of `floats` `f32` values, if a single allocation can hold them.
pub(crate) fn floats_to_bytes(floats: usize) -> Option<usize> {
    let bytes = floats.checked_mul(std::mem::size_of::<f32>())?;
    // Allocations are limited to isize::MAX bytes.
    (bytes <= isize::MAX as usize).then_some(bytes)
}

/// Checks the size limits and asks the allocator for `bytes` up front, so
/// that a request the system cannot satisfy fails here with an error rather
/// than aborting the process in the middle of the computation. The probe is
/// released immediately; with memory overcommit it touches no pages.
pub(crate) fn check_memory(bytes: usize, limit: Option<usize>) -> Result<(), TedError> {
    if let Some(limit) = limit {
        if bytes > limit {
            return Err(TedError::MemoryLimitExceeded {
                estimated: bytes,
                limit,
            });
        }
    }
    let mut probe: Vec<u8> = Vec::new();
    probe
        .try_reserve_exact(bytes)
        .map_err(|_| TedError::AllocationFailed { bytes })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_rejects_unrepresentable_sizes() {
        assert!(estimated_peak_bytes(1000, 1000).is_some());
        assert_eq!(estimated_peak_bytes(1 << 23, 1 << 23), None);
        assert_eq!(estimated_peak_bytes(usize::MAX, 1), None);
    }

    #[test]
    fn check_memory_reports_limits_and_failures() {
        assert!(check_memory(1024, None).is_ok());
        assert!(matches!(
            check_memory(2048, Some(1024)),
            Err(TedError::MemoryLimitExceeded { .. })
        ));
        assert!(matches!(
            check_memory(isize::MAX as usize, None),
            Err(TedError::AllocationFailed { .. })
        ));
    }
}
