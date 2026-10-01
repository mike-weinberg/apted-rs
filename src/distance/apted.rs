//! Port of `distance.APTED`.
//!
//! - Optimal strategy with all paths.
//! - Single-node single path function supports currently only unit cost.
//! - Two-node single path function not included.
//! - \Delta^L and \Delta^R based on Zhang and Shasha's algorithm for executing
//!   left and right paths (as in [3]). If only left and right paths are used
//!   in the strategy, the memory usage is reduced by one quadratic array.
//! - For any other path \Delta^A from [1] is used.
//!
//! References:
//! - [1] M. Pawlik and N. Augsten. Efficient Computation of the Tree Edit
//!   Distance. ACM Transactions on Database Systems (TODS) 40(1). 2015.
//! - [2] M. Pawlik and N. Augsten. Tree edit distance: Robust and memory-
//!   efficient. Information Systems 56. 2016.
//! - [3] M. Pawlik and N. Augsten. RTED: A Robust Algorithm for the Tree Edit
//!   Distance. PVLDB 5(4). 2011.
//!
//! The port keeps the structure of the Java code, including `f32` arithmetic
//! and the reuse of the `delta` matrix for both the strategy and the subtree
//! distances, so that results match the reference implementation exactly.

use super::matrix::Matrix;
use crate::cost_model::CostModel;
use crate::error::{check_memory, estimated_peak_bytes, floats_to_bytes, TedError};
use crate::node::{Node, NodeIndexer};

/// Identifier of left path type.
const LEFT: u8 = 0;
/// Identifier of right path type.
const RIGHT: u8 = 1;
/// Identifier of inner path type.
const INNER: u8 = 2;

/// `(float) 0x7fffffffffffffffL` in the Java code.
const INF: f32 = i64::MAX as f32;

/// The APTED algorithm with cost model `C` for node data `D`.
///
/// An `APTED` value borrows both trees from the call to
/// [`compute_edit_distance`](Self::compute_edit_distance) until it is
/// dropped, and keeps the intermediate results that
/// [`compute_edit_mapping`](Self::compute_edit_mapping) needs. Create one per
/// pair of trees, or reuse it sequentially.
///
/// ```
/// use apted::{BracketStringInputParser, StringUnitCostModel, APTED};
///
/// let p = BracketStringInputParser::new();
/// let (t1, t2) = (p.from_string("{a{b}{c}}"), p.from_string("{a{b{d}}}"));
/// let mut apted = APTED::new(StringUnitCostModel);
/// assert_eq!(apted.compute_edit_distance(&t1, &t2), 2.0);
/// ```
pub struct APTED<'a, C, D> {
    cost_model: C,
    it1: Option<NodeIndexer<'a, D>>,
    it2: Option<NodeIndexer<'a, D>>,
    size1: i32,
    size2: i32,
    /// The distance matrix [1, Sections 3.4,8.2,8.3]. Holds the strategy
    /// first, then intermediate distances between pairs of subtrees.
    delta: Matrix,
    /// Number of subproblems encountered while computing the distance
    /// [1, Section 10].
    counter: u64,
    /// Distance of the current pair, once computed; the mapping needs it.
    distance: Option<f32>,
    /// Optional cap on the estimated peak memory, in bytes.
    memory_limit: Option<usize>,
    /// Bytes checked up front for the current pair (see [`Work`]).
    checked_bytes: usize,
}

/// Mutable state of one distance computation. Kept apart from the node
/// indexers so both indexers can be borrowed while the state is mutated.
struct Work<'c, C> {
    cost_model: &'c C,
    delta: Matrix,
    /// One of distance arrays to store intermediate distances in spfA.
    q: Vec<f32>,
    /// Array used in the algorithm before [1] (see [1, Section 8.4]).
    fn_: Vec<i32>,
    /// Array used in the algorithm before [1] (see [1, Section 8.4]).
    ft: Vec<i32>,
    /// Reused forest distance matrix of spfL and spfR (row-major).
    forestdist: Vec<f32>,
    counter: u64,
    /// Memory limit for spfA's tables, from [`APTED::with_memory_limit`].
    memory_limit: Option<usize>,
    /// Bytes already checked against the limit and the allocator; spfA
    /// tables up to this size need no further check.
    checked_bytes: usize,
    /// Set when spfA cannot allocate its tables; stops the computation.
    error: Option<TedError>,
}

/// Per-column data of the spfL/spfR inner loop.
struct Column<'n, D> {
    node: &'n Node<D>,
    ins: f32,
    /// Offset of this column's node in `delta`, completed by the row's
    /// offset (accounts for swapped input order).
    delta: usize,
    leaf: usize,
    is_tree: bool,
}

/// A strategy path being processed by [`Work::gted`]: the subtree pair,
/// which tree the path lies in, and how far the walk up the path and over
/// the children of the current path node has got.
struct GtedFrame {
    subtree1: i32,
    subtree2: i32,
    in_tree2: bool,
    path_type: u8,
    /// First node of the path (a leaf), as passed to spfA.
    path_start: i32,
    /// Current node of the walk up the path.
    path_node: i32,
    /// Next child of `path_node`'s parent to visit.
    child_idx: usize,
}

impl<'a, C: CostModel<D>, D> APTED<'a, C, D> {
    /// Creates the algorithm with `cost_model`, with no memory limit.
    pub fn new(cost_model: C) -> Self {
        Self {
            cost_model,
            it1: None,
            it2: None,
            size1: 0,
            size2: 0,
            delta: Matrix::default(),
            counter: 0,
            distance: None,
            memory_limit: None,
            checked_bytes: 0,
        }
    }

    /// Makes [`Self::try_compute_edit_distance`] fail with
    /// [`TedError::MemoryLimitExceeded`] instead of computing when the
    /// estimated peak memory ([`crate::estimated_peak_bytes`]) is above
    /// `bytes`. Use it when tree sizes come from untrusted input.
    pub fn with_memory_limit(mut self, bytes: usize) -> Self {
        self.memory_limit = Some(bytes);
        self
    }

    /// Computes the tree edit distance between the source and destination
    /// trees using APTED (Pawlik and Augsten; see the references in the crate
    /// documentation).
    ///
    /// Panics if the trees are too large or the memory is not available;
    /// see [`Self::try_compute_edit_distance`] for the checks.
    pub fn compute_edit_distance(&mut self, t1: &'a Node<D>, t2: &'a Node<D>) -> f32 {
        self.try_compute_edit_distance(t1, t2)
            .unwrap_or_else(|e| panic!("{e}"))
    }

    /// Computes the tree edit distance, first checking that the trees are
    /// small enough for the algorithm's `f32` node ids, that the estimated
    /// peak memory is within the limit set with [`Self::with_memory_limit`],
    /// and that the allocator can provide it. The checks run once, before
    /// any work, so an oversized input returns an error instead of aborting
    /// the process on allocation failure.
    pub fn try_compute_edit_distance(
        &mut self,
        t1: &'a Node<D>,
        t2: &'a Node<D>,
    ) -> Result<f32, TedError> {
        let (size1, size2) = (t1.node_count(), t2.node_count());
        let bytes =
            estimated_peak_bytes(size1, size2).ok_or(TedError::TooLarge { size1, size2 })?;
        check_memory(bytes, self.memory_limit)?;
        self.init(t1, t2);
        self.checked_bytes = bytes;
        let it1 = self.it1.as_ref().unwrap();
        let it2 = self.it2.as_ref().unwrap();
        // Determine the optimal strategy with the heuristic from
        // [2, Section 5.3].
        let delta = if it1.lchl < it1.rchl {
            compute_opt_strategy_post_l(it1, it2)
        } else {
            compute_opt_strategy_post_r(it1, it2)
        };
        let d = self.run(delta)?;
        self.distance = Some(d);
        Ok(d)
    }

    /// Testing-only entry point, not part of the supported API: computes TED with a fixed path type in the
    /// strategy to trigger a specific single-path function. `spf_type` is
    /// 0 for left paths (spfL) and 1 for right paths (spfR).
    #[doc(hidden)]
    pub fn compute_edit_distance_spf_test(
        &mut self,
        t1: &'a Node<D>,
        t2: &'a Node<D>,
        spf_type: i32,
    ) -> f32 {
        self.init(t1, t2);
        let it1 = self.it1.as_ref().unwrap();
        let mut delta = Matrix::new(self.size1 as usize, self.size2 as usize);
        for (i, row) in delta.rows_mut().enumerate() {
            for cell in row.iter_mut() {
                if spf_type == LEFT as i32 {
                    *cell = (it1.pre_l_to_lld(i as i32) + 1) as f32;
                } else if spf_type == RIGHT as i32 {
                    *cell = (it1.pre_l_to_rld(i as i32) + 1) as f32;
                }
            }
        }
        let d = self.run(delta).unwrap_or_else(|e| panic!("{e}"));
        self.distance = Some(d);
        d
    }

    /// Indexes both input trees and stores their sizes. Testing-only, not
    /// part of the supported API.
    #[doc(hidden)]
    pub fn init(&mut self, t1: &'a Node<D>, t2: &'a Node<D>) {
        let it1 = NodeIndexer::new(t1, &self.cost_model);
        let it2 = NodeIndexer::new(t2, &self.cost_model);
        self.size1 = it1.size();
        self.size2 = it2.size();
        self.it1 = Some(it1);
        self.it2 = Some(it2);
        // Results of an earlier pair no longer apply.
        self.delta = Matrix::default();
        self.distance = None;
        self.counter = 0;
    }

    /// Initialises the structures for the distance computation and runs GTED
    /// with the strategy stored in `delta`.
    fn run(&mut self, delta: Matrix) -> Result<f32, TedError> {
        let it1 = self.it1.as_ref().unwrap();
        let it2 = self.it2.as_ref().unwrap();
        let max_size = self.size1.max(self.size2) as usize + 1;
        let mut work = Work {
            cost_model: &self.cost_model,
            delta,
            q: vec![0.0; max_size],
            fn_: vec![0; max_size + 1],
            ft: vec![0; max_size + 1],
            forestdist: Vec::new(),
            counter: 0,
            memory_limit: self.memory_limit,
            checked_bytes: self.checked_bytes,
            error: None,
        };
        work.ted_init(it1, it2);
        let result = work.gted(it1, it2);
        self.counter = work.counter;
        self.delta = work.delta;
        match work.error {
            Some(e) => Err(e),
            None => Ok(result),
        }
    }

    /// Number of subproblems encountered in the last distance computation.
    /// Used by the benchmark; not part of the supported API.
    #[doc(hidden)]
    pub fn counter(&self) -> u64 {
        self.counter
    }

    /// Computes the edit mapping between the two input trees. The distance
    /// must be computed first (distances of subtree pairs are required).
    ///
    /// Returns pairs of postorder ids (starting with 1) of mapped nodes.
    /// Deleted and inserted nodes are mapped to 0.
    ///
    /// Panics in the cases where [`Self::try_compute_edit_mapping`] returns
    /// an error.
    pub fn compute_edit_mapping(&self) -> Vec<[i32; 2]> {
        self.try_compute_edit_mapping()
            .unwrap_or_else(|e| panic!("{e}"))
    }

    /// Computes the edit mapping, or an error if no distance was computed
    /// for the current trees, the distance is NaN, or the mapping's
    /// (n+1)·(m+1) table cannot be allocated.
    pub fn try_compute_edit_mapping(&self) -> Result<Vec<[i32; 2]>, TedError> {
        let d = self.distance.ok_or(TedError::DistanceNotComputed)?;
        if d.is_nan() {
            return Err(TedError::NotANumber);
        }
        let (it1, it2) = match (&self.it1, &self.it2) {
            (Some(it1), Some(it2)) => (it1, it2),
            _ => return Err(TedError::DistanceNotComputed),
        };
        let floats = (self.size1 as usize + 1) * (self.size2 as usize + 1);
        check_memory(floats * std::mem::size_of::<f32>(), self.memory_limit)?;
        let cm = &self.cost_model;
        let (size1, size2) = (self.size1, self.size2);
        let mut forestdist = vec![vec![0.0f32; size2 as usize + 1]; size1 as usize + 1];
        let mut root_node_pair = true;
        self.forest_dist(it1, it2, size1, size2, &mut forestdist);
        // Pairs are collected in push order and reversed at the end; the Java
        // implementation pushes to the front of a linked list.
        let mut edit_mapping: Vec<[i32; 2]> = Vec::new();
        let mut tree_pairs: Vec<[i32; 2]> = vec![[size1, size2]];

        while let Some([last_row, last_col]) = tree_pairs.pop() {
            if !root_node_pair {
                self.forest_dist(it1, it2, last_row, last_col, &mut forestdist);
            }
            root_node_pair = false;

            let first_row = it1.post_l_to_lld[(last_row - 1) as usize];
            let first_col = it2.post_l_to_lld[(last_col - 1) as usize];
            let mut row = last_row;
            let mut col = last_col;
            while row > first_row || col > first_col {
                let (r, c) = (row as usize, col as usize);
                // With an empty subforest on one side the only possible
                // operation is an insertion or deletion. The float
                // comparisons below would pick it too, except for NaN costs,
                // where they would fall through to an out-of-range index.
                if row == first_row {
                    edit_mapping.push([0, col]);
                    col -= 1;
                } else if col == first_col {
                    edit_mapping.push([row, 0]);
                    row -= 1;
                } else if row > first_row
                    && forestdist[r - 1][c] + cm.del(it1.post_l_to_node(row - 1))
                        == forestdist[r][c]
                {
                    // Node with postorder id `row` is deleted from the source.
                    edit_mapping.push([row, 0]);
                    row -= 1;
                } else if col > first_col
                    && forestdist[r][c - 1] + cm.ins(it2.post_l_to_node(col - 1))
                        == forestdist[r][c]
                {
                    // Node with postorder id `col` is inserted into the destination.
                    edit_mapping.push([0, col]);
                    col -= 1;
                } else if it1.post_l_to_lld[r - 1] == it1.post_l_to_lld[(last_row - 1) as usize]
                    && it2.post_l_to_lld[c - 1] == it2.post_l_to_lld[(last_col - 1) as usize]
                {
                    // Both subforests are trees: map the nodes.
                    edit_mapping.push([row, col]);
                    row -= 1;
                    col -= 1;
                } else {
                    // Push the subtree pair and continue with the forest to
                    // its left.
                    tree_pairs.push([row, col]);
                    row = it1.post_l_to_lld[r - 1];
                    col = it2.post_l_to_lld[c - 1];
                }
            }
        }
        edit_mapping.reverse();
        Ok(edit_mapping)
    }

    /// Recalculates distances between subforests of two subtrees, used by the
    /// mapping computation to track back the origin of minimum values. Based
    /// on Zhang and Shasha's algorithm. `i` and `j` are postorder ids
    /// starting with 1.
    fn forest_dist(
        &self,
        ted1: &NodeIndexer<'a, D>,
        ted2: &NodeIndexer<'a, D>,
        i: i32,
        j: i32,
        forestdist: &mut [Vec<f32>],
    ) {
        let cm = &self.cost_model;
        let li = ted1.post_l_to_lld[(i - 1) as usize];
        let lj = ted2.post_l_to_lld[(j - 1) as usize];
        forestdist[li as usize][lj as usize] = 0.0;
        for di in (li + 1)..=i {
            let d = di as usize;
            forestdist[d][lj as usize] =
                forestdist[d - 1][lj as usize] + cm.del(ted1.post_l_to_node(di - 1));
            for dj in (lj + 1)..=j {
                let e = dj as usize;
                forestdist[li as usize][e] =
                    forestdist[li as usize][e - 1] + cm.ins(ted2.post_l_to_node(dj - 1));
                let cost_ren = cm.ren(ted1.post_l_to_node(di - 1), ted2.post_l_to_node(dj - 1));
                let a = forestdist[d - 1][e] + cm.del(ted1.post_l_to_node(di - 1));
                let b = forestdist[d][e - 1] + cm.ins(ted2.post_l_to_node(dj - 1));
                let c = if ted1.post_l_to_lld[d - 1] == li && ted2.post_l_to_lld[e - 1] == lj {
                    forestdist[d - 1][e - 1] + cost_ren
                } else {
                    forestdist[ted1.post_l_to_lld[d - 1] as usize]
                        [ted2.post_l_to_lld[e - 1] as usize]
                        + self.delta[ted1.post_l_to_pre_l[d - 1] as usize]
                            [ted2.post_l_to_pre_l[e - 1] as usize]
                        + cost_ren
                };
                forestdist[d][e] = java_min(java_min(a, b), c);
            }
        }
    }

    /// Sums up the cost of every operation in an edit mapping.
    pub fn mapping_cost(&self, mapping: &[[i32; 2]]) -> f32 {
        let it1 = self
            .it1
            .as_ref()
            .expect("compute the distance before the mapping cost");
        let it2 = self
            .it2
            .as_ref()
            .expect("compute the distance before the mapping cost");
        let cm = &self.cost_model;
        let mut cost = 0.0f32;
        for m in mapping {
            if m[0] == 0 {
                cost += cm.ins(it2.post_l_to_node(m[1] - 1));
            } else if m[1] == 0 {
                cost += cm.del(it1.post_l_to_node(m[0] - 1));
            } else {
                cost += cm.ren(it1.post_l_to_node(m[0] - 1), it2.post_l_to_node(m[1] - 1));
            }
        }
        cost
    }
}

/// `Math.min` for floats, ignoring NaN handling (costs are never NaN).
fn java_min(a: f32, b: f32) -> f32 {
    if a <= b {
        a
    } else {
        b
    }
}

/// Pool of reusable cost rows for the strategy computation. Row 0 is the
/// shared all-zero row of leaves, which is only ever read.
struct RowPool {
    l: Vec<Vec<f32>>,
    r: Vec<Vec<f32>>,
    i: Vec<Vec<f32>>,
    free: Vec<usize>,
}

impl RowPool {
    fn new(width: usize) -> Self {
        Self {
            l: vec![vec![0.0; width]],
            r: vec![vec![0.0; width]],
            i: vec![vec![0.0; width]],
            free: Vec::new(),
        }
    }

    fn take(&mut self, width: usize) -> usize {
        if let Some(k) = self.free.pop() {
            k
        } else {
            self.l.push(vec![0.0; width]);
            self.r.push(vec![0.0; width]);
            self.i.push(vec![0.0; width]);
            self.l.len() - 1
        }
    }

    fn release(&mut self, k: usize) {
        self.l[k].fill(0.0);
        self.r[k].fill(0.0);
        self.i[k].fill(0.0);
        self.free.push(k);
    }
}

/// Computes the optimal strategy using left-to-right postorder traversal of
/// the nodes [2, Algorithm 1].
pub(crate) fn compute_opt_strategy_post_l<D>(
    it1: &NodeIndexer<'_, D>,
    it2: &NodeIndexer<'_, D>,
) -> Matrix {
    let size1 = it1.size() as usize;
    let size2 = it2.size() as usize;
    let mut strategy = Matrix::new(size1, size2);
    // Row of the pool holding the costs of each node (by postorder id).
    let mut cost1: Vec<Option<usize>> = vec![None; size1];
    let mut pool = RowPool::new(size2);
    let mut cost2_l = vec![0.0f32; size2];
    let mut cost2_r = vec![0.0f32; size2];
    let mut cost2_i = vec![0.0f32; size2];
    let mut cost2_path = vec![0i32; size2];
    let path_id_offset = size1 as i32;

    for v in 0..size1 {
        let v_in_pre_l = it1.post_l_to_pre_l[v];
        let vp = v_in_pre_l as usize;
        let is_v_leaf = it1.is_leaf(v_in_pre_l);
        let parent_v_pre_l = it1.parents[vp];
        let parent_v_post_l = if parent_v_pre_l != -1 {
            it1.pre_l_to_post_l[parent_v_pre_l as usize] as usize
        } else {
            usize::MAX
        };

        let size_v = it1.sizes[vp];
        // Left path's id: the leftmost leaf node.
        let left_path_v = -(it1.pre_r_to_pre_l[(it1.pre_l_to_pre_r[vp] + size_v - 1) as usize] + 1);
        // Right path's id: the rightmost leaf node.
        let right_path_v = v_in_pre_l + size_v - 1 + 1;
        let kr_sum_v = it1.pre_l_to_kr_sum[vp];
        let revkr_sum_v = it1.pre_l_to_rev_kr_sum[vp];
        let desc_sum_v = it1.pre_l_to_desc_sum[vp];

        if is_v_leaf {
            cost1[v] = Some(0);
            for i in 0..size2 {
                strategy[vp][it2.post_l_to_pre_l[i] as usize] = v_in_pre_l as f32;
            }
        }
        let row_v = cost1[v].expect("children processed before parent");

        if parent_v_pre_l != -1 && cost1[parent_v_post_l].is_none() {
            cost1[parent_v_post_l] = Some(pool.take(size2));
        }
        let row_p = if parent_v_pre_l != -1 {
            cost1[parent_v_post_l]
        } else {
            None
        };

        cost2_l.fill(0.0);
        cost2_r.fill(0.0);
        cost2_i.fill(0.0);
        cost2_path.fill(0);

        for w in 0..size2 {
            let w_in_pre_l = it2.post_l_to_pre_l[w];
            let wp = w_in_pre_l as usize;
            let parent_w_pre_l = it2.parents[wp];
            let parent_w_post_l = if parent_w_pre_l != -1 {
                it2.pre_l_to_post_l[parent_w_pre_l as usize] as usize
            } else {
                usize::MAX
            };

            let size_w = it2.sizes[wp];
            if it2.is_leaf(w_in_pre_l) {
                cost2_l[w] = 0.0;
                cost2_r[w] = 0.0;
                cost2_i[w] = 0.0;
                cost2_path[w] = w_in_pre_l;
            }
            let mut min_cost = INF;
            let mut strategy_path = -1i32;

            if size_v <= 1 || size_w <= 1 {
                // Use new single-path functions for small subtrees.
                min_cost = size_v.max(size_w) as f32;
            } else {
                let mut tmp_cost = size_v as f32 * it2.pre_l_to_kr_sum[wp] + pool.l[row_v][w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = left_path_v;
                }
                tmp_cost = size_v as f32 * it2.pre_l_to_rev_kr_sum[wp] + pool.r[row_v][w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = right_path_v;
                }
                tmp_cost = size_v as f32 * it2.pre_l_to_desc_sum[wp] + pool.i[row_v][w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = strategy[vp][wp] as i32 + 1;
                }
                tmp_cost = size_w as f32 * kr_sum_v + cost2_l[w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = -(it2.pre_r_to_pre_l
                        [(it2.pre_l_to_pre_r[wp] + size_w - 1) as usize]
                        + path_id_offset
                        + 1);
                }
                tmp_cost = size_w as f32 * revkr_sum_v + cost2_r[w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = w_in_pre_l + size_w - 1 + path_id_offset + 1;
                }
                tmp_cost = size_w as f32 * desc_sum_v + cost2_i[w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = cost2_path[w] + path_id_offset + 1;
                }
            }

            if let Some(p) = row_p {
                let pv = parent_v_pre_l as usize;
                pool.r[p][w] += min_cost;
                let tmp_cost = -min_cost + pool.i[row_v][w];
                if tmp_cost < pool.i[p][w] {
                    pool.i[p][w] = tmp_cost;
                    strategy[pv][wp] = strategy[vp][wp];
                }
                if it1.node_type_r[vp] {
                    pool.i[p][w] += pool.r[p][w];
                    pool.r[p][w] += pool.r[row_v][w] - min_cost;
                }
                if it1.node_type_l[vp] {
                    pool.l[p][w] += pool.l[row_v][w];
                } else {
                    pool.l[p][w] += min_cost;
                }
            }
            if parent_w_pre_l != -1 {
                let pw = parent_w_post_l;
                cost2_r[pw] += min_cost;
                let tmp_cost = -min_cost + cost2_i[w];
                if tmp_cost < cost2_i[pw] {
                    cost2_i[pw] = tmp_cost;
                    cost2_path[pw] = cost2_path[w];
                }
                if it2.node_type_r[wp] {
                    cost2_i[pw] += cost2_r[pw];
                    cost2_r[pw] += cost2_r[w] - min_cost;
                }
                if it2.node_type_l[wp] {
                    cost2_l[pw] += cost2_l[w];
                } else {
                    cost2_l[pw] += min_cost;
                }
            }
            strategy[vp][wp] = strategy_path as f32;
        }

        if !it1.is_leaf(v_in_pre_l) {
            pool.release(row_v);
        }
    }
    strategy
}

/// Computes the optimal strategy using right-to-left postorder traversal of
/// the nodes [2, Algorithm 1].
pub(crate) fn compute_opt_strategy_post_r<D>(
    it1: &NodeIndexer<'_, D>,
    it2: &NodeIndexer<'_, D>,
) -> Matrix {
    let size1 = it1.size() as usize;
    let size2 = it2.size() as usize;
    let mut strategy = Matrix::new(size1, size2);
    // Row of the pool holding the costs of each node (by preorder id).
    let mut cost1: Vec<Option<usize>> = vec![None; size1];
    let mut pool = RowPool::new(size2);
    let mut cost2_l = vec![0.0f32; size2];
    let mut cost2_r = vec![0.0f32; size2];
    let mut cost2_i = vec![0.0f32; size2];
    let mut cost2_path = vec![0i32; size2];
    let path_id_offset = size1 as i32;

    for v in (0..size1).rev() {
        let vi = v as i32;
        let is_v_leaf = it1.is_leaf(vi);
        let parent_v = it1.parents[v];

        let size_v = it1.sizes[v];
        let left_path_v =
            -(it1.pre_r_to_pre_l[(it1.pre_l_to_pre_r[v] + it1.sizes[v] - 1) as usize] + 1);
        let right_path_v = vi + it1.sizes[v] - 1 + 1;
        let kr_sum_v = it1.pre_l_to_kr_sum[v];
        let revkr_sum_v = it1.pre_l_to_rev_kr_sum[v];
        let desc_sum_v = it1.pre_l_to_desc_sum[v];

        if is_v_leaf {
            cost1[v] = Some(0);
            for i in 0..size2 {
                strategy[v][i] = vi as f32;
            }
        }
        let row_v = cost1[v].expect("children processed before parent");

        if parent_v != -1 && cost1[parent_v as usize].is_none() {
            cost1[parent_v as usize] = Some(pool.take(size2));
        }
        let row_p = if parent_v != -1 {
            cost1[parent_v as usize]
        } else {
            None
        };

        cost2_l.fill(0.0);
        cost2_r.fill(0.0);
        cost2_i.fill(0.0);
        cost2_path.fill(0);

        for w in (0..size2).rev() {
            let wi = w as i32;
            let size_w = it2.sizes[w];
            if it2.is_leaf(wi) {
                cost2_l[w] = 0.0;
                cost2_r[w] = 0.0;
                cost2_i[w] = 0.0;
                cost2_path[w] = wi;
            }
            let mut min_cost = INF;
            let mut strategy_path = -1i32;

            if size_v <= 1 || size_w <= 1 {
                min_cost = size_v.max(size_w) as f32;
            } else {
                let mut tmp_cost = size_v as f32 * it2.pre_l_to_kr_sum[w] + pool.l[row_v][w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = left_path_v;
                }
                tmp_cost = size_v as f32 * it2.pre_l_to_rev_kr_sum[w] + pool.r[row_v][w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = right_path_v;
                }
                tmp_cost = size_v as f32 * it2.pre_l_to_desc_sum[w] + pool.i[row_v][w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = strategy[v][w] as i32 + 1;
                }
                tmp_cost = size_w as f32 * kr_sum_v + cost2_l[w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = -(it2.pre_r_to_pre_l
                        [(it2.pre_l_to_pre_r[w] + size_w - 1) as usize]
                        + path_id_offset
                        + 1);
                }
                tmp_cost = size_w as f32 * revkr_sum_v + cost2_r[w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = wi + size_w - 1 + path_id_offset + 1;
                }
                tmp_cost = size_w as f32 * desc_sum_v + cost2_i[w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = cost2_path[w] + path_id_offset + 1;
                }
            }

            if let Some(p) = row_p {
                let pv = parent_v as usize;
                pool.l[p][w] += min_cost;
                let tmp_cost = -min_cost + pool.i[row_v][w];
                if tmp_cost < pool.i[p][w] {
                    pool.i[p][w] = tmp_cost;
                    strategy[pv][w] = strategy[v][w];
                }
                if it1.node_type_l[v] {
                    pool.i[p][w] += pool.l[p][w];
                    pool.l[p][w] += pool.l[row_v][w] - min_cost;
                }
                if it1.node_type_r[v] {
                    pool.r[p][w] += pool.r[row_v][w];
                } else {
                    pool.r[p][w] += min_cost;
                }
            }
            let parent_w = it2.parents[w];
            if parent_w != -1 {
                let pw = parent_w as usize;
                cost2_l[pw] += min_cost;
                let tmp_cost = -min_cost + cost2_i[w];
                if tmp_cost < cost2_i[pw] {
                    cost2_i[pw] = tmp_cost;
                    cost2_path[pw] = cost2_path[w];
                }
                if it2.node_type_l[w] {
                    cost2_i[pw] += cost2_l[pw];
                    cost2_l[pw] += cost2_l[w] - min_cost;
                }
                if it2.node_type_r[w] {
                    cost2_r[pw] += cost2_r[w];
                } else {
                    cost2_r[pw] += min_cost;
                }
            }
            strategy[v][w] = strategy_path as f32;
        }

        if !it1.is_leaf(vi) {
            pool.release(row_v);
        }
    }
    strategy
}

impl<'c, C> Work<'c, C> {
    /// After the optimal strategy is computed, initialises distances of
    /// deleting and inserting subtrees without their root nodes.
    fn ted_init<D>(&mut self, it1: &NodeIndexer<'_, D>, it2: &NodeIndexer<'_, D>)
    where
        C: CostModel<D>,
    {
        self.counter = 0;
        for x in 0..it1.size() as usize {
            let size_x = it1.sizes[x];
            for y in 0..it2.size() as usize {
                let size_y = it2.sizes[y];
                // The order of the input trees is the original one here.
                if size_x == 1 && size_y == 1 {
                    self.delta[x][y] = 0.0;
                } else if size_x == 1 {
                    self.delta[x][y] =
                        it2.pre_l_to_sum_ins_cost[y] - self.cost_model.ins(it2.pre_l_to_node[y]);
                } else if size_y == 1 {
                    self.delta[x][y] =
                        it1.pre_l_to_sum_del_cost[x] - self.cost_model.del(it1.pre_l_to_node[x]);
                }
            }
        }
    }

    /// spf1: single path function for the case when one of the subtrees is a
    /// single node [2, Section 6.1, Algorithm 2]. Renames may cost less than
    /// deletion plus insertion, so Formula 4 in [2] is adapted. The subtrees
    /// are always passed in the original input order.
    fn spf1<D>(
        &self,
        ni1: &NodeIndexer<'_, D>,
        subtree_root_node1: i32,
        ni2: &NodeIndexer<'_, D>,
        subtree_root_node2: i32,
    ) -> f32
    where
        C: CostModel<D>,
    {
        let cm = self.cost_model;
        let r1 = subtree_root_node1 as usize;
        let r2 = subtree_root_node2 as usize;
        let subtree_size1 = ni1.sizes[r1];
        let subtree_size2 = ni2.sizes[r2];
        if subtree_size1 == 1 && subtree_size2 == 1 {
            let n1 = ni1.pre_l_to_node[r1];
            let n2 = ni2.pre_l_to_node[r2];
            let max_cost = cm.del(n1) + cm.ins(n2);
            let ren_cost = cm.ren(n1, n2);
            return if ren_cost < max_cost {
                ren_cost
            } else {
                max_cost
            };
        }
        if subtree_size1 == 1 {
            let n1 = ni1.pre_l_to_node[r1];
            let mut cost = ni2.pre_l_to_sum_ins_cost[r2];
            let max_cost = cost + cm.del(n1);
            // Upstream starts at `cost`, which undercounts when renames are
            // expensive; see tests/regressions.rs.
            let mut min_ren_minus_ins = f32::INFINITY;
            for i in r2..r2 + subtree_size2 as usize {
                let n2 = ni2.pre_l_to_node[i];
                let node_ren_minus_ins = cm.ren(n1, n2) - cm.ins(n2);
                if node_ren_minus_ins < min_ren_minus_ins {
                    min_ren_minus_ins = node_ren_minus_ins;
                }
            }
            cost += min_ren_minus_ins;
            return if cost < max_cost { cost } else { max_cost };
        }
        if subtree_size2 == 1 {
            let n2 = ni2.pre_l_to_node[r2];
            let mut cost = ni1.pre_l_to_sum_del_cost[r1];
            let max_cost = cost + cm.ins(n2);
            // Upstream starts at `cost`; see tests/regressions.rs.
            let mut min_ren_minus_del = f32::INFINITY;
            for i in r1..r1 + subtree_size1 as usize {
                let n1 = ni1.pre_l_to_node[i];
                let node_ren_minus_del = cm.ren(n1, n2) - cm.del(n1);
                if node_ren_minus_del < min_ren_minus_del {
                    min_ren_minus_del = node_ren_minus_del;
                }
            }
            cost += min_ren_minus_del;
            return if cost < max_cost { cost } else { max_cost };
        }
        -1.0
    }

    /// GTED [1, Section 3.4]: decomposes the trees along the strategy paths
    /// and dispatches to the single-path functions.
    ///
    /// The Java version recurses into every subtree hanging off a strategy
    /// path before running the path's single-path function. Here an explicit
    /// stack of [`GtedFrame`]s replaces the recursion so that deep trees
    /// cannot overflow the call stack; the order of single-path calls, and
    /// therefore every result, is unchanged.
    fn gted<D>(&mut self, it1: &NodeIndexer<'_, D>, it2: &NodeIndexer<'_, D>) -> f32
    where
        C: CostModel<D>,
    {
        let mut stack = match self.gted_enter(it1, it2, it1.current_node(), it2.current_node()) {
            Ok(frame) => vec![frame],
            Err(d) => return d,
        };
        loop {
            let f = stack.last_mut().expect("loop exits when the stack empties");
            let (it, subtree) = if f.in_tree2 {
                (it2, f.subtree2)
            } else {
                (it1, f.subtree1)
            };
            // Next subtree hanging off the path, walking up from its end.
            let mut next = None;
            loop {
                let parent = it.parents[f.path_node as usize];
                if parent < subtree {
                    break;
                }
                let children = &it.children[parent as usize];
                while let Some(&child) = children.get(f.child_idx) {
                    f.child_idx += 1;
                    if child != f.path_node {
                        next = Some(child);
                        break;
                    }
                }
                if next.is_some() {
                    break;
                }
                f.path_node = parent;
                f.child_idx = 0;
            }
            if let Some(child) = next {
                let (s1, s2) = if f.in_tree2 {
                    (f.subtree1, child)
                } else {
                    (child, f.subtree2)
                };
                if let Ok(frame) = self.gted_enter(it1, it2, s1, s2) {
                    stack.push(frame);
                }
                continue;
            }

            // Every relevant subtree is done: run this path's function.
            let f = stack.pop().expect("stack is non-empty");
            it1.set_current_node(f.subtree1);
            it2.set_current_node(f.subtree2);
            // The flag says whether the input subtrees were swapped compared
            // to the original input order [1, Section 3.4].
            let d = match (f.in_tree2, f.path_type) {
                (false, LEFT) => self.spf_l(it1, it2, false),
                (false, RIGHT) => self.spf_r(it1, it2, false),
                (false, _) => self.spf_a(it1, it2, f.path_start, f.path_type, false),
                (true, LEFT) => self.spf_l(it2, it1, true),
                (true, RIGHT) => self.spf_r(it2, it1, true),
                (true, _) => self.spf_a(it2, it1, f.path_start, f.path_type, true),
            };
            if stack.is_empty() || self.error.is_some() {
                return d;
            }
        }
    }

    /// Starts GTED on the subtree pair (`subtree1`, `subtree2`): a pair with
    /// a single-node tree is solved at once with spf1 (`Err` carries its
    /// distance); otherwise returns the frame that walks its strategy path.
    fn gted_enter<D>(
        &mut self,
        it1: &NodeIndexer<'_, D>,
        it2: &NodeIndexer<'_, D>,
        subtree1: i32,
        subtree2: i32,
    ) -> Result<GtedFrame, f32>
    where
        C: CostModel<D>,
    {
        it1.set_current_node(subtree1);
        it2.set_current_node(subtree2);
        let subtree_size1 = it1.sizes[subtree1 as usize];
        let subtree_size2 = it2.sizes[subtree2 as usize];
        if subtree_size1 == 1 || subtree_size2 == 1 {
            return Err(self.spf1(it1, subtree1, it2, subtree2));
        }
        let strategy_path_id = self.delta[subtree1 as usize][subtree2 as usize] as i32;
        let path_id_offset = it1.size();
        let path_node = strategy_path_id.abs() - 1;
        let (in_tree2, path_node, root, size) = if path_node < path_id_offset {
            (false, path_node, subtree1, subtree_size1)
        } else {
            (true, path_node - path_id_offset, subtree2, subtree_size2)
        };
        Ok(GtedFrame {
            subtree1,
            subtree2,
            in_tree2,
            path_type: get_strategy_path_type(strategy_path_id, path_id_offset, root, size),
            path_start: path_node,
            path_node,
            child_idx: 0,
        })
    }

    /// Reads `delta` for node `f` of the left-hand tree and node `g` of the
    /// right-hand tree, accounting for swapped input order.
    #[inline]
    fn delta_at(&self, f: i32, g: i32, trees_swapped: bool) -> f32 {
        if trees_swapped {
            self.delta.get(g as usize, f as usize)
        } else {
            self.delta.get(f as usize, g as usize)
        }
    }

    /// Writes `delta` for node `f` of the left-hand tree and node `g` of the
    /// right-hand tree, accounting for swapped input order.
    #[inline]
    fn set_delta(&mut self, f: i32, g: i32, trees_swapped: bool, value: f32) {
        if trees_swapped {
            self.delta.set(g as usize, f as usize, value);
        } else {
            self.delta.set(f as usize, g as usize, value);
        }
    }

    /// The single-path function spfA [1, Sections 7 and 8]. Used strictly for
    /// inner paths, although it also executes correctly for left and right
    /// paths. `path_id` is the preorder id of the strategy path's leaf node.
    ///
    /// The `s` and `t` matrices are stored row-major in flat vectors.
    fn spf_a<D>(
        &mut self,
        it1: &NodeIndexer<'_, D>,
        it2: &NodeIndexer<'_, D>,
        path_id: i32,
        path_type: u8,
        trees_swapped: bool,
    ) -> f32
    where
        C: CostModel<D>,
    {
        let cm = self.cost_model;
        // Cost-model helpers that account for swapped input order.
        let del_f = |n: &Node<D>| if trees_swapped { cm.ins(n) } else { cm.del(n) };
        let ins_g = |n: &Node<D>| if trees_swapped { cm.del(n) } else { cm.ins(n) };
        let ren_fg = |f: &Node<D>, g: &Node<D>| {
            if trees_swapped {
                cm.ren(g, f)
            } else {
                cm.ren(f, g)
            }
        };
        let sum_del_f = |x: i32| {
            if trees_swapped {
                it1.pre_l_to_sum_ins_cost[x as usize]
            } else {
                it1.pre_l_to_sum_del_cost[x as usize]
            }
        };
        let sum_ins_g = |y: i32| {
            if trees_swapped {
                it2.pre_l_to_sum_del_cost[y as usize]
            } else {
                it2.pre_l_to_sum_ins_cost[y as usize]
            }
        };

        let it2nodes = &it2.pre_l_to_node;
        let it2sizes = &it2.sizes;
        let it1sizes = &it1.sizes;
        let it1parents = &it1.parents;
        let it2parents = &it2.parents;
        let it1pre_l_to_pre_r = &it1.pre_l_to_pre_r;
        let it2pre_l_to_pre_r = &it2.pre_l_to_pre_r;
        let it1pre_r_to_pre_l = &it1.pre_r_to_pre_l;
        let it2pre_r_to_pre_l = &it2.pre_r_to_pre_l;
        let current_subtree_pre_l1 = it1.current_node();
        let current_subtree_pre_l2 = it2.current_node();

        // Forest sizes and costs, summed up incrementally.
        let mut current_forest_size1: i32 = 0;
        let mut tmp_forest_size1: i32;
        let mut current_forest_cost1: f32 = 0.0;
        let mut current_forest_cost2: f32;
        let mut tmp_forest_cost1: f32;

        let subtree_size2 = it2.sizes[current_subtree_pre_l2 as usize];
        let subtree_size1 = it1.sizes[current_subtree_pre_l1 as usize];
        let w = (subtree_size2 + 1) as usize; // Row width of both s and t.
                                              // t is (max(n, m) + 1)² in the worst case, beyond the estimate
                                              // checked up front, so check it (with s) before allocating.
        let tables = w
            .checked_mul(w)
            .and_then(|t| t.checked_add((subtree_size1 + 1) as usize * w))
            .and_then(floats_to_bytes);
        let checked = match tables {
            // Within what was checked up front: no second probe needed.
            Some(bytes) if bytes <= self.checked_bytes => Ok(()),
            Some(bytes) => check_memory(bytes, self.memory_limit).map(|()| {
                self.checked_bytes = bytes;
            }),
            None => Err(TedError::AllocationFailed { bytes: usize::MAX }),
        };
        if let Err(e) = checked {
            self.error = Some(e);
            return f32::NAN;
        }
        let mut t = vec![0.0f32; w * w];
        let mut s = vec![0.0f32; (subtree_size1 + 1) as usize * w];
        let si = |row: i32, col: i32| row as usize * w + col as usize;
        let mut min_cost: f32 = -1.0;
        // sp1, sp2 and sp3 are the three elements of the minimum in the
        // recursive formula [1, Figure 12].
        let mut sp1: f32 = 0.0;
        let mut sp2: f32;
        let mut sp3: f32;
        let mut start_path_node: i32 = -1;
        let mut end_path_node: i32 = path_id;
        let mut it1_pre_l_off: i32;
        let it2_pre_l_off: i32 = current_subtree_pre_l2;
        let mut it1_pre_r_off: i32;
        let it2_pre_r_off: i32 = it2pre_l_to_pre_r[it2_pre_l_off as usize];
        let fn_last = self.fn_.len() - 1;

        // Loop A [1, Algorithm 3] - walk up the path.
        while end_path_node >= current_subtree_pre_l1 {
            it1_pre_l_off = end_path_node;
            it1_pre_r_off = it1pre_l_to_pre_r[end_path_node as usize];
            let mut r_f_last: i32 = -1;
            let mut l_f_last: i32;
            let end_path_node_in_pre_r = it1pre_l_to_pre_r[end_path_node as usize];
            let start_path_node_in_pre_r = if start_path_node == -1 {
                i32::MAX
            } else {
                it1pre_l_to_pre_r[start_path_node as usize]
            };
            let parent_of_end_path_node = it1parents[end_path_node as usize];
            let parent_of_end_path_node_in_pre_r = if parent_of_end_path_node == -1 {
                i32::MAX
            } else {
                it1pre_l_to_pre_r[parent_of_end_path_node as usize]
            };
            let left_part = start_path_node - end_path_node > 1;
            let right_part =
                start_path_node >= 0 && start_path_node_in_pre_r - end_path_node_in_pre_r > 1;

            // Deal with nodes to the left of the path.
            if path_type == RIGHT || path_type == INNER && left_part {
                let (r_f_first, l_f_first) = if start_path_node == -1 {
                    (end_path_node_in_pre_r, end_path_node)
                } else {
                    (start_path_node_in_pre_r, start_path_node - 1)
                };
                if !right_part {
                    r_f_last = end_path_node_in_pre_r;
                }
                let r_g_last = it2pre_l_to_pre_r[current_subtree_pre_l2 as usize];
                let r_g_first = (r_g_last + subtree_size2) - 1;
                l_f_last = if right_part {
                    end_path_node + 1
                } else {
                    end_path_node
                };
                self.fn_[fn_last] = -1;
                for i in current_subtree_pre_l2..current_subtree_pre_l2 + subtree_size2 {
                    self.fn_[i as usize] = -1;
                    self.ft[i as usize] = -1;
                }
                // Store the current size and cost of the forest in F.
                tmp_forest_size1 = current_forest_size1;
                tmp_forest_cost1 = current_forest_cost1;
                // Loop B [1, Algorithm 3] - for all nodes in G.
                let mut r_g = r_g_first;
                while r_g >= r_g_last {
                    let l_g_first = it2pre_r_to_pre_l[r_g as usize];
                    let r_g_in_pre_l = it2pre_r_to_pre_l[r_g as usize];
                    let r_g_minus1_in_pre_l =
                        if r_g <= it2pre_l_to_pre_r[current_subtree_pre_l2 as usize] {
                            i32::MAX
                        } else {
                            it2pre_r_to_pre_l[(r_g - 1) as usize]
                        };
                    let parent_of_r_g_in_pre_l = it2parents[r_g_in_pre_l as usize];
                    // Decide on the last lG node for Loop D [1, Algorithm 3].
                    let l_g_last = if path_type == RIGHT {
                        if l_g_first == current_subtree_pre_l2
                            || r_g_minus1_in_pre_l != parent_of_r_g_in_pre_l
                        {
                            l_g_first
                        } else {
                            it2parents[l_g_first as usize] + 1
                        }
                    } else if l_g_first == current_subtree_pre_l2 {
                        l_g_first
                    } else {
                        current_subtree_pre_l2 + 1
                    };
                    self.update_fn_array(
                        it2.pre_l_to_ln[l_g_first as usize],
                        l_g_first,
                        current_subtree_pre_l2,
                    );
                    self.update_ft_array(it2.pre_l_to_ln[l_g_first as usize], l_g_first);
                    let mut r_f = r_f_first;
                    // Reset size and cost of the forest in F.
                    current_forest_size1 = tmp_forest_size1;
                    current_forest_cost1 = tmp_forest_cost1;
                    // Loop C [1, Algorithm 3] - for all nodes to the left of
                    // the path node.
                    let mut l_f = l_f_first;
                    while l_f >= l_f_last {
                        // Fix the rF node.
                        if l_f == l_f_last && !right_part {
                            r_f = r_f_last;
                        }
                        let l_f_node = it1.pre_l_to_node[l_f as usize];
                        // Increment size and cost of F forest by node lF.
                        current_forest_size1 += 1;
                        current_forest_cost1 += del_f(l_f_node);
                        // Reset size and cost of forest in G to subtree
                        // G_lGfirst. The Java original also increments the G
                        // forest size in loop D, but it only ever reads the
                        // size right here, so only `size == 1` is kept.
                        let g_forest_is_single_node = it2sizes[l_g_first as usize] == 1;
                        current_forest_cost2 = sum_ins_g(l_g_first);
                        let l_f_in_pre_r = it1pre_l_to_pre_r[l_f as usize];
                        let f_forest_is_tree = l_f_in_pre_r == r_f;
                        let l_f_subtree_size = it1sizes[l_f as usize];
                        let l_f_is_consecutive_node_of_current_path_node =
                            start_path_node - l_f == 1;
                        let l_f_is_left_sibling_of_current_path_node =
                            l_f + l_f_subtree_size == start_path_node;
                        let sp1s_row = (l_f + 1) - it1_pre_l_off;
                        let sp2s_row = l_f - it1_pre_l_off;
                        let mut sp3s_row = 0;
                        let swrite_row = l_f - it1_pre_l_off;
                        // Sources of sp1 and sp3 [1, Figures 12,13]:
                        // 1 = s array, 2 = t array / subtree, 3 = cost sums.
                        let mut sp1source: u8 = 1;
                        let mut sp3source: u8 = 1;
                        if f_forest_is_tree {
                            if l_f_subtree_size == 1 {
                                sp1source = 3;
                            } else if l_f_is_consecutive_node_of_current_path_node {
                                sp1source = 2;
                            }
                            sp3 = 0.0;
                            sp3source = 2;
                        } else {
                            if l_f_is_consecutive_node_of_current_path_node {
                                sp1source = 2;
                            }
                            sp3 = current_forest_cost1 - sum_del_f(l_f);
                            if l_f_is_left_sibling_of_current_path_node {
                                sp3source = 3;
                            }
                        }
                        if sp3source == 1 {
                            sp3s_row = (l_f + l_f_subtree_size) - it1_pre_l_off;
                        }
                        // Go to first lG.
                        let mut l_g = l_g_first;
                        // sp1, sp2, sp3 for the first node in Loop D.
                        match sp1source {
                            1 => sp1 = s[si(sp1s_row, l_g - it2_pre_l_off)],
                            2 => sp1 = t[si(l_g - it2_pre_l_off, r_g - it2_pre_r_off)],
                            3 => sp1 = current_forest_cost2,
                            _ => unreachable!(),
                        }
                        sp1 += del_f(l_f_node);
                        min_cost = sp1;
                        if g_forest_is_single_node {
                            sp2 = current_forest_cost1;
                        } else {
                            sp2 = self.q[l_f as usize];
                        }
                        sp2 += ins_g(it2nodes[l_g as usize]);
                        if sp2 < min_cost {
                            min_cost = sp2;
                        }
                        if sp3 < min_cost {
                            sp3 += self.delta_at(l_f, l_g, trees_swapped);
                            if sp3 < min_cost {
                                sp3 += ren_fg(l_f_node, it2nodes[l_g as usize]);
                                if sp3 < min_cost {
                                    min_cost = sp3;
                                }
                            }
                        }
                        s[si(swrite_row, l_g - it2_pre_l_off)] = min_cost;
                        // Go to next lG.
                        l_g = self.ft[l_g as usize];
                        self.counter += 1;
                        // Loop D [1, Algorithm 3] - for all nodes to the left
                        // of rG.
                        while l_g >= l_g_last {
                            let lgu = l_g as usize;
                            // Increment cost of G forest by node lG.
                            current_forest_cost2 += ins_g(it2nodes[lgu]);
                            match sp1source {
                                1 => sp1 = s[si(sp1s_row, l_g - it2_pre_l_off)] + del_f(l_f_node),
                                2 => {
                                    sp1 = t[si(l_g - it2_pre_l_off, r_g - it2_pre_r_off)]
                                        + del_f(l_f_node)
                                }
                                3 => sp1 = current_forest_cost2 + del_f(l_f_node),
                                _ => unreachable!(),
                            }
                            sp2 = s[si(sp2s_row, self.fn_[lgu] - it2_pre_l_off)]
                                + ins_g(it2nodes[lgu]);
                            min_cost = sp1;
                            if sp2 < min_cost {
                                min_cost = sp2;
                            }
                            sp3 = self.delta_at(l_f, l_g, trees_swapped);
                            if sp3 < min_cost {
                                match sp3source {
                                    1 => {
                                        sp3 += s[si(
                                            sp3s_row,
                                            self.fn_[((l_g + it2sizes[lgu]) - 1) as usize]
                                                - it2_pre_l_off,
                                        )]
                                    }
                                    2 => sp3 += current_forest_cost2 - sum_ins_g(l_g),
                                    3 => {
                                        sp3 += t[si(
                                            self.fn_[((l_g + it2sizes[lgu]) - 1) as usize]
                                                - it2_pre_l_off,
                                            r_g - it2_pre_r_off,
                                        )]
                                    }
                                    _ => unreachable!(),
                                }
                                if sp3 < min_cost {
                                    sp3 += ren_fg(l_f_node, it2nodes[lgu]);
                                    if sp3 < min_cost {
                                        min_cost = sp3;
                                    }
                                }
                            }
                            s[si(swrite_row, l_g - it2_pre_l_off)] = min_cost;
                            l_g = self.ft[lgu];
                            self.counter += 1;
                        }
                        l_f -= 1;
                    }
                    if r_g_minus1_in_pre_l == parent_of_r_g_in_pre_l {
                        if !right_part {
                            if left_part {
                                let v = s[si(
                                    (l_f_last + 1) - it1_pre_l_off,
                                    (r_g_minus1_in_pre_l + 1) - it2_pre_l_off,
                                )];
                                self.set_delta(
                                    end_path_node,
                                    parent_of_r_g_in_pre_l,
                                    trees_swapped,
                                    v,
                                );
                            }
                            if end_path_node > 0
                                && end_path_node == parent_of_end_path_node + 1
                                && end_path_node_in_pre_r == parent_of_end_path_node_in_pre_r + 1
                            {
                                let v = s[si(
                                    l_f_last - it1_pre_l_off,
                                    (r_g_minus1_in_pre_l + 1) - it2_pre_l_off,
                                )];
                                self.set_delta(
                                    parent_of_end_path_node,
                                    parent_of_r_g_in_pre_l,
                                    trees_swapped,
                                    v,
                                );
                            }
                        }
                        let mut l_f = l_f_first;
                        while l_f >= l_f_last {
                            self.q[l_f as usize] = s[si(
                                l_f - it1_pre_l_off,
                                (parent_of_r_g_in_pre_l + 1) - it2_pre_l_off,
                            )];
                            l_f -= 1;
                        }
                    }
                    let mut l_g = l_g_first;
                    while l_g >= l_g_last {
                        t[si(l_g - it2_pre_l_off, r_g - it2_pre_r_off)] =
                            s[si(l_f_last - it1_pre_l_off, l_g - it2_pre_l_off)];
                        l_g = self.ft[l_g as usize];
                    }
                    r_g -= 1;
                }
            }
            // Deal with nodes to the right of the path.
            if path_type == LEFT
                || path_type == INNER && right_part
                || path_type == INNER && !left_part && !right_part
            {
                let r_f_first;
                let l_f_first;
                if start_path_node == -1 {
                    l_f_first = end_path_node;
                    r_f_first = it1pre_l_to_pre_r[end_path_node as usize];
                } else {
                    r_f_first = it1pre_l_to_pre_r[start_path_node as usize] - 1;
                    l_f_first = end_path_node + 1;
                }
                l_f_last = end_path_node;
                let l_g_last = current_subtree_pre_l2;
                let l_g_first = (l_g_last + subtree_size2) - 1;
                r_f_last = it1pre_l_to_pre_r[end_path_node as usize];
                self.fn_[fn_last] = -1;
                for i in current_subtree_pre_l2..current_subtree_pre_l2 + subtree_size2 {
                    self.fn_[i as usize] = -1;
                    self.ft[i as usize] = -1;
                }
                // Store size and cost of the current forest in F.
                tmp_forest_size1 = current_forest_size1;
                tmp_forest_cost1 = current_forest_cost1;
                // Loop B' [1, Algorithm 3] - for all nodes in G.
                let mut l_g = l_g_first;
                while l_g >= l_g_last {
                    let lgu = l_g as usize;
                    let r_g_first = it2pre_l_to_pre_r[lgu];
                    self.update_fn_array(
                        it2.pre_r_to_ln[r_g_first as usize],
                        r_g_first,
                        it2pre_l_to_pre_r[current_subtree_pre_l2 as usize],
                    );
                    self.update_ft_array(it2.pre_r_to_ln[r_g_first as usize], r_g_first);
                    let mut l_f = l_f_first;
                    let l_g_minus1_in_pre_r = if l_g <= current_subtree_pre_l2 {
                        i32::MAX
                    } else {
                        it2pre_l_to_pre_r[lgu - 1]
                    };
                    let parent_of_l_g = it2parents[lgu];
                    let parent_of_l_g_in_pre_r = if parent_of_l_g == -1 {
                        -1
                    } else {
                        it2pre_l_to_pre_r[parent_of_l_g as usize]
                    };
                    // Reset size and cost of forest in F.
                    current_forest_size1 = tmp_forest_size1;
                    current_forest_cost1 = tmp_forest_cost1;
                    let r_g_last = if path_type == LEFT {
                        if l_g == current_subtree_pre_l2
                            || it2.children[parent_of_l_g as usize][0] != l_g
                        {
                            r_g_first
                        } else {
                            it2pre_l_to_pre_r[parent_of_l_g as usize] + 1
                        }
                    } else if r_g_first == it2pre_l_to_pre_r[current_subtree_pre_l2 as usize] {
                        r_g_first
                    } else {
                        it2pre_l_to_pre_r[current_subtree_pre_l2 as usize]
                    };
                    // Loop C' [1, Algorithm 3] - for all nodes to the right of
                    // the path node.
                    let mut r_f = r_f_first;
                    while r_f >= r_f_last {
                        if r_f == r_f_last {
                            l_f = l_f_last;
                        }
                        let r_f_in_pre_l = it1pre_r_to_pre_l[r_f as usize];
                        let r_f_node = it1.pre_l_to_node[r_f_in_pre_l as usize];
                        // Increment size and cost of F forest by node rF.
                        current_forest_size1 += 1;
                        current_forest_cost1 += del_f(r_f_node);
                        // Reset size and cost of G forest to G_lG (only
                        // `size == 1` is read; see loop C).
                        let g_forest_is_single_node = it2sizes[lgu] == 1;
                        current_forest_cost2 = sum_ins_g(l_g);
                        let r_f_subtree_size = it1sizes[r_f_in_pre_l as usize];
                        let (
                            r_f_is_consecutive_node_of_current_path_node,
                            r_f_is_right_sibling_of_current_path_node,
                        ) = if start_path_node > 0 {
                            (
                                start_path_node_in_pre_r - r_f == 1,
                                r_f + r_f_subtree_size == start_path_node_in_pre_r,
                            )
                        } else {
                            (false, false)
                        };
                        let f_forest_is_tree = r_f_in_pre_l == l_f;
                        let sp1s_row = (r_f + 1) - it1_pre_r_off;
                        let sp2s_row = r_f - it1_pre_r_off;
                        let mut sp3s_row = 0;
                        let swrite_row = r_f - it1_pre_r_off;
                        let sp1t_row = l_g - it2_pre_l_off;
                        let sp3t_row = l_g - it2_pre_l_off;
                        let mut sp1source: u8 = 1;
                        let mut sp3source: u8 = 1;
                        if f_forest_is_tree {
                            if r_f_subtree_size == 1 {
                                sp1source = 3;
                            } else if r_f_is_consecutive_node_of_current_path_node {
                                sp1source = 2;
                            }
                            sp3 = 0.0;
                            sp3source = 2;
                        } else {
                            if r_f_is_consecutive_node_of_current_path_node {
                                sp1source = 2;
                            }
                            sp3 = current_forest_cost1 - sum_del_f(r_f_in_pre_l);
                            if r_f_is_right_sibling_of_current_path_node {
                                sp3source = 3;
                            }
                        }
                        if sp3source == 1 {
                            sp3s_row = (r_f + r_f_subtree_size) - it1_pre_r_off;
                        }
                        if g_forest_is_single_node {
                            sp2 = current_forest_cost1;
                        } else {
                            sp2 = self.q[r_f as usize];
                        }
                        let mut r_g = r_g_first;
                        let r_g_first_in_pre_l = it2pre_r_to_pre_l[r_g_first as usize];
                        match sp1source {
                            1 => sp1 = s[si(sp1s_row, r_g - it2_pre_r_off)],
                            2 => sp1 = t[si(sp1t_row, r_g - it2_pre_r_off)],
                            3 => sp1 = current_forest_cost2,
                            _ => unreachable!(),
                        }
                        sp1 += del_f(r_f_node);
                        min_cost = sp1;
                        sp2 += ins_g(it2nodes[r_g_first_in_pre_l as usize]);
                        if sp2 < min_cost {
                            min_cost = sp2;
                        }
                        if sp3 < min_cost {
                            sp3 += self.delta_at(r_f_in_pre_l, r_g_first_in_pre_l, trees_swapped);
                            if sp3 < min_cost {
                                sp3 += ren_fg(r_f_node, it2nodes[r_g_first_in_pre_l as usize]);
                                if sp3 < min_cost {
                                    min_cost = sp3;
                                }
                            }
                        }
                        s[si(swrite_row, r_g - it2_pre_r_off)] = min_cost;
                        r_g = self.ft[r_g as usize];
                        self.counter += 1;
                        // Loop D' [1, Algorithm 3] - for all nodes to the
                        // right of lG.
                        while r_g >= r_g_last {
                            let r_g_in_pre_l = it2pre_r_to_pre_l[r_g as usize];
                            let rgp = r_g_in_pre_l as usize;
                            // Increment cost of G forest by node rG.
                            current_forest_cost2 += ins_g(it2nodes[rgp]);
                            match sp1source {
                                1 => sp1 = s[si(sp1s_row, r_g - it2_pre_r_off)] + del_f(r_f_node),
                                2 => sp1 = t[si(sp1t_row, r_g - it2_pre_r_off)] + del_f(r_f_node),
                                3 => sp1 = current_forest_cost2 + del_f(r_f_node),
                                _ => unreachable!(),
                            }
                            sp2 = s[si(sp2s_row, self.fn_[r_g as usize] - it2_pre_r_off)]
                                + ins_g(it2nodes[rgp]);
                            min_cost = sp1;
                            if sp2 < min_cost {
                                min_cost = sp2;
                            }
                            sp3 = self.delta_at(r_f_in_pre_l, r_g_in_pre_l, trees_swapped);
                            if sp3 < min_cost {
                                match sp3source {
                                    1 => {
                                        sp3 += s[si(
                                            sp3s_row,
                                            self.fn_[((r_g + it2sizes[rgp]) - 1) as usize]
                                                - it2_pre_r_off,
                                        )]
                                    }
                                    2 => sp3 += current_forest_cost2 - sum_ins_g(r_g_in_pre_l),
                                    3 => {
                                        sp3 += t[si(
                                            sp3t_row,
                                            self.fn_[((r_g + it2sizes[rgp]) - 1) as usize]
                                                - it2_pre_r_off,
                                        )]
                                    }
                                    _ => unreachable!(),
                                }
                                if sp3 < min_cost {
                                    sp3 += ren_fg(r_f_node, it2nodes[rgp]);
                                    if sp3 < min_cost {
                                        min_cost = sp3;
                                    }
                                }
                            }
                            s[si(swrite_row, r_g - it2_pre_r_off)] = min_cost;
                            r_g = self.ft[r_g as usize];
                            self.counter += 1;
                        }
                        r_f -= 1;
                    }
                    if l_g > current_subtree_pre_l2 && l_g - 1 == parent_of_l_g {
                        if right_part {
                            let v = s[si(
                                (r_f_last + 1) - it1_pre_r_off,
                                (l_g_minus1_in_pre_r + 1) - it2_pre_r_off,
                            )];
                            self.set_delta(end_path_node, parent_of_l_g, trees_swapped, v);
                        }
                        if end_path_node > 0
                            && end_path_node == parent_of_end_path_node + 1
                            && end_path_node_in_pre_r == parent_of_end_path_node_in_pre_r + 1
                        {
                            let v = s[si(
                                r_f_last - it1_pre_r_off,
                                (l_g_minus1_in_pre_r + 1) - it2_pre_r_off,
                            )];
                            self.set_delta(
                                parent_of_end_path_node,
                                parent_of_l_g,
                                trees_swapped,
                                v,
                            );
                        }
                        let mut r_f = r_f_first;
                        while r_f >= r_f_last {
                            self.q[r_f as usize] = s[si(
                                r_f - it1_pre_r_off,
                                (parent_of_l_g_in_pre_r + 1) - it2_pre_r_off,
                            )];
                            r_f -= 1;
                        }
                    }
                    let mut r_g = r_g_first;
                    while r_g >= r_g_last {
                        t[si(l_g - it2_pre_l_off, r_g - it2_pre_r_off)] =
                            s[si(r_f_last - it1_pre_r_off, r_g - it2_pre_r_off)];
                        r_g = self.ft[r_g as usize];
                    }
                    l_g -= 1;
                }
            }
            // Walk up the path by one node.
            start_path_node = end_path_node;
            end_path_node = it1parents[end_path_node as usize];
        }
        let _ = (current_forest_size1, sp1);
        min_cost
    }

    /// Single-path function for left paths [1, Sections 3.3,3.4,3.5].
    fn spf_l<D>(
        &mut self,
        it1: &NodeIndexer<'_, D>,
        it2: &NodeIndexer<'_, D>,
        trees_swapped: bool,
    ) -> f32
    where
        C: CostModel<D>,
    {
        self.spf_lr(it1, it2, trees_swapped, false)
    }

    /// Single-path function for right paths [1, Sections 3.3,3.4,3.5].
    fn spf_r<D>(
        &mut self,
        it1: &NodeIndexer<'_, D>,
        it2: &NodeIndexer<'_, D>,
        trees_swapped: bool,
    ) -> f32
    where
        C: CostModel<D>,
    {
        self.spf_lr(it1, it2, trees_swapped, true)
    }

    /// spfL (`right == false`) and spfR (`right == true`). Both run Zhang and
    /// Shasha's algorithm; spfR uses right-to-left orders and rightmost
    /// leaves. In the left-hand subtree only the root is a keyroot, so the
    /// distance between it and every keyroot of the right-hand subtree is
    /// computed.
    fn spf_lr<D>(
        &mut self,
        it1: &NodeIndexer<'_, D>,
        it2: &NodeIndexer<'_, D>,
        trees_swapped: bool,
        right: bool,
    ) -> f32
    where
        C: CostModel<D>,
    {
        let c2 = it2.current_node();
        let c1 = it1.current_node();
        let size1 = it1.sizes[c1 as usize] as usize;
        let size2 = it2.sizes[c2 as usize] as usize;
        let mut key_roots = vec![-1i32; size2];
        let first_key_root = compute_key_roots(it2, c2, &mut key_roots, right);
        let width = size2 + 1;
        let mut forestdist = std::mem::take(&mut self.forestdist);
        forestdist.clear();
        forestdist.resize((size1 + 1) * width, 0.0);
        for i in (0..first_key_root).rev() {
            self.tree_edit_dist(
                it1,
                it2,
                c1,
                key_roots[i],
                &mut forestdist,
                width,
                trees_swapped,
                right,
            );
        }
        let result = forestdist[size1 * width + size2];
        self.forestdist = forestdist;
        result
    }

    /// Core of spfL/spfR: fills `forestdist` (row-major, `width` columns)
    /// with distances of subforest pairs, in left-to-right postorder for
    /// spfL and right-to-left postorder for spfR. Per-row and per-column
    /// values are computed once, outside the inner loop.
    #[allow(clippy::too_many_arguments)]
    fn tree_edit_dist<D>(
        &mut self,
        it1: &NodeIndexer<'_, D>,
        it2: &NodeIndexer<'_, D>,
        it1subtree: i32,
        it2subtree: i32,
        forestdist: &mut [f32],
        width: usize,
        trees_swapped: bool,
        right: bool,
    ) where
        C: CostModel<D>,
    {
        let cm = self.cost_model;
        let del_f = |n: &Node<D>| if trees_swapped { cm.ins(n) } else { cm.del(n) };
        let ins_g = |n: &Node<D>| if trees_swapped { cm.del(n) } else { cm.ins(n) };
        // post_to_pre, post_to_leaf (lld or rld), pre_to_post per direction.
        let (post_to_pre1, leaf1, pre_to_post1) = if right {
            (
                &it1.post_r_to_pre_l,
                &it1.post_r_to_rld,
                &it1.pre_l_to_post_r,
            )
        } else {
            (
                &it1.post_l_to_pre_l,
                &it1.post_l_to_lld,
                &it1.pre_l_to_post_l,
            )
        };
        let (post_to_pre2, leaf2, pre_to_post2) = if right {
            (
                &it2.post_r_to_pre_l,
                &it2.post_r_to_rld,
                &it2.pre_l_to_post_r,
            )
        } else {
            (
                &it2.post_l_to_pre_l,
                &it2.post_l_to_lld,
                &it2.pre_l_to_post_l,
            )
        };
        let i = pre_to_post1[it1subtree as usize];
        let j = pre_to_post2[it2subtree as usize];
        // Offsets so that forestdist indices start at 0.
        let ioff = leaf1[i as usize] - 1;
        let joff = leaf2[j as usize] - 1;
        let rows = (i - ioff) as usize;
        let cols = (j - joff) as usize;
        let leaf_i = leaf1[i as usize];
        let leaf_j = leaf2[j as usize];
        let stride = self.delta.cols();

        // Column data, indexed by j1 (index 0 unused).
        let mut cols_buf = Vec::with_capacity(cols + 1);
        cols_buf.push(Column {
            node: it2.pre_l_to_node[0],
            ins: 0.0,
            delta: 0,
            leaf: 0,
            is_tree: false,
        });
        for j1 in 1..=cols as i32 {
            let post = (j1 + joff) as usize;
            let pre = post_to_pre2[post];
            let node = it2.pre_l_to_node[pre as usize];
            cols_buf.push(Column {
                node,
                ins: ins_g(node),
                delta: if trees_swapped {
                    pre as usize * stride
                } else {
                    pre as usize
                },
                leaf: (leaf2[post] - 1 - joff) as usize,
                is_tree: leaf2[post] == leaf_j,
            });
        }

        let delta = self.delta.as_mut_slice();
        forestdist[0] = 0.0;
        for i1 in 1..=rows {
            let node = it1.pre_l_to_node[post_to_pre1[(i1 as i32 + ioff) as usize] as usize];
            forestdist[i1 * width] = forestdist[(i1 - 1) * width] + del_f(node);
        }
        for j1 in 1..=cols {
            forestdist[j1] = forestdist[j1 - 1] + cols_buf[j1].ins;
        }
        for i1 in 1..=rows {
            let post1 = (i1 as i32 + ioff) as usize;
            let f = post_to_pre1[post1];
            let n1 = it1.pre_l_to_node[f as usize];
            let del1 = del_f(n1);
            let row1_is_tree = leaf1[post1] == leaf_i;
            let leaf_row = (leaf1[post1] - 1 - ioff) as usize * width;
            let row = i1 * width;
            let prev = row - width;
            // Rows before i1 (including the previous row and the rows of
            // leaf offsets) are read-only while row i1 is written.
            let (before, rest) = forestdist.split_at_mut(row);
            let cur = &mut rest[..=cols];
            let prev_row = &before[prev..=prev + cols];
            let delta_row = if trees_swapped {
                f as usize
            } else {
                f as usize * stride
            };
            let mut left = cur[0];
            for j1 in 1..=cols {
                let col = &cols_buf[j1];
                let u = if trees_swapped {
                    cm.ren(col.node, n1)
                } else {
                    cm.ren(n1, col.node)
                };
                let diag = prev_row[j1 - 1];
                let da = prev_row[j1] + del1;
                let db = left + col.ins;
                let di = delta_row + col.delta;
                let dc = if row1_is_tree && col.is_tree {
                    // Both subforests are subtrees.
                    delta[di] = diag;
                    diag + u
                } else {
                    before[leaf_row + col.leaf] + delta[di] + u
                };
                let v = if da >= db {
                    if db >= dc {
                        dc
                    } else {
                        db
                    }
                } else if da >= dc {
                    dc
                } else {
                    da
                };
                cur[j1] = v;
                left = v;
            }
        }
        self.counter += (rows * cols) as u64;
    }

    fn update_fn_array(&mut self, ln_for_node: i32, node: i32, current_subtree_pre_l: i32) {
        let last = self.fn_.len() - 1;
        if ln_for_node >= current_subtree_pre_l {
            self.fn_[node as usize] = self.fn_[ln_for_node as usize];
            self.fn_[ln_for_node as usize] = node;
        } else {
            self.fn_[node as usize] = self.fn_[last];
            self.fn_[last] = node;
        }
    }

    fn update_ft_array(&mut self, ln_for_node: i32, node: i32) {
        self.ft[node as usize] = ln_for_node;
        if self.fn_[node as usize] > -1 {
            let f = self.fn_[node as usize] as usize;
            self.ft[f] = node;
        }
    }
}

/// Stores the keyroot nodes of the subtree rooted at `subtree_root_node`
/// into `key_roots`: the root, then recursively every sibling of a node on
/// its left path (right path if `right`), in the order of the Java
/// recursion. Returns the number of keyroots stored. Iterative, so deep
/// trees cannot overflow the call stack.
fn compute_key_roots<D>(
    it2: &NodeIndexer<'_, D>,
    subtree_root_node: i32,
    key_roots: &mut [i32],
    right: bool,
) -> usize {
    let leaf = |node: i32| {
        if right {
            it2.pre_l_to_rld(node)
        } else {
            it2.pre_l_to_lld(node)
        }
    };
    // (subtree root, current path node, next child of its parent to visit)
    let mut stack = vec![(subtree_root_node, leaf(subtree_root_node), 0usize)];
    key_roots[0] = subtree_root_node;
    let mut index = 1;
    while let Some((root, path_node, child_idx)) = stack.last_mut() {
        if *path_node <= *root {
            stack.pop();
            continue;
        }
        let parent = it2.parents[*path_node as usize];
        match it2.children[parent as usize][*child_idx..]
            .iter()
            .position(|&c| c != *path_node)
        {
            Some(offset) => {
                // Every sibling of a path node is a keyroot.
                let child = it2.children[parent as usize][*child_idx + offset];
                *child_idx += offset + 1;
                key_roots[index] = child;
                index += 1;
                stack.push((child, leaf(child), 0));
            }
            None => {
                *path_node = parent;
                *child_idx = 0;
            }
        }
    }
    index
}

/// Decodes a raw path id from the strategy into its type (LEFT, RIGHT,
/// INNER).
fn get_strategy_path_type(
    path_id_with_path_id_offset: i32,
    path_id_offset: i32,
    current_root_node_pre_l: i32,
    current_subtree_size: i32,
) -> u8 {
    if path_id_with_path_id_offset < 0 {
        return LEFT;
    }
    let mut path_id = path_id_with_path_id_offset.abs() - 1;
    if path_id >= path_id_offset {
        path_id -= path_id_offset;
    }
    if path_id == (current_root_node_pre_l + current_subtree_size) - 1 {
        return RIGHT;
    }
    INNER
}
