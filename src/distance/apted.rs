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

use crate::cost_model::CostModel;
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
pub struct APTED<'a, C, D> {
    cost_model: C,
    it1: Option<NodeIndexer<'a, D>>,
    it2: Option<NodeIndexer<'a, D>>,
    size1: i32,
    size2: i32,
    /// The distance matrix [1, Sections 3.4,8.2,8.3]. Holds the strategy
    /// first, then intermediate distances between pairs of subtrees.
    delta: Vec<Vec<f32>>,
    /// Number of subproblems encountered while computing the distance
    /// [1, Section 10].
    counter: u64,
}

/// Mutable state of one distance computation. Kept apart from the node
/// indexers so both indexers can be borrowed while the state is mutated.
struct Work<'c, C> {
    cost_model: &'c C,
    delta: Vec<Vec<f32>>,
    /// One of distance arrays to store intermediate distances in spfA.
    q: Vec<f32>,
    /// Array used in the algorithm before [1] (see [1, Section 8.4]).
    fn_: Vec<i32>,
    /// Array used in the algorithm before [1] (see [1, Section 8.4]).
    ft: Vec<i32>,
    counter: u64,
}

impl<'a, C: CostModel<D>, D> APTED<'a, C, D> {
    pub fn new(cost_model: C) -> Self {
        Self {
            cost_model,
            it1: None,
            it2: None,
            size1: 0,
            size2: 0,
            delta: Vec::new(),
            counter: 0,
        }
    }

    /// Computes the tree edit distance between the source and destination
    /// trees using APTED [1,2].
    pub fn compute_edit_distance(&mut self, t1: &'a Node<D>, t2: &'a Node<D>) -> f32 {
        self.init(t1, t2);
        let it1 = self.it1.as_ref().unwrap();
        let it2 = self.it2.as_ref().unwrap();
        // Determine the optimal strategy with the heuristic from
        // [2, Section 5.3].
        let delta = if it1.lchl < it1.rchl {
            compute_opt_strategy_post_l(it1, it2)
        } else {
            compute_opt_strategy_post_r(it1, it2)
        };
        self.run(delta)
    }

    /// Testing-only entry point: computes TED with a fixed path type in the
    /// strategy to trigger a specific single-path function. `spf_type` is
    /// 0 for left paths (spfL) and 1 for right paths (spfR).
    pub fn compute_edit_distance_spf_test(
        &mut self,
        t1: &'a Node<D>,
        t2: &'a Node<D>,
        spf_type: i32,
    ) -> f32 {
        self.init(t1, t2);
        let it1 = self.it1.as_ref().unwrap();
        let mut delta = vec![vec![0.0f32; self.size2 as usize]; self.size1 as usize];
        for (i, row) in delta.iter_mut().enumerate() {
            for cell in row.iter_mut() {
                if spf_type == LEFT as i32 {
                    *cell = (it1.pre_l_to_lld(i as i32) + 1) as f32;
                } else if spf_type == RIGHT as i32 {
                    *cell = (it1.pre_l_to_rld(i as i32) + 1) as f32;
                }
            }
        }
        self.run(delta)
    }

    /// Indexes both input trees and stores their sizes.
    pub fn init(&mut self, t1: &'a Node<D>, t2: &'a Node<D>) {
        let it1 = NodeIndexer::new(t1, &self.cost_model);
        let it2 = NodeIndexer::new(t2, &self.cost_model);
        self.size1 = it1.size();
        self.size2 = it2.size();
        self.it1 = Some(it1);
        self.it2 = Some(it2);
    }

    /// Initialises the structures for the distance computation and runs GTED
    /// with the strategy stored in `delta`.
    fn run(&mut self, delta: Vec<Vec<f32>>) -> f32 {
        let it1 = self.it1.as_ref().unwrap();
        let it2 = self.it2.as_ref().unwrap();
        let max_size = self.size1.max(self.size2) as usize + 1;
        let mut work = Work {
            cost_model: &self.cost_model,
            delta,
            q: vec![0.0; max_size],
            fn_: vec![0; max_size + 1],
            ft: vec![0; max_size + 1],
            counter: 0,
        };
        work.ted_init(it1, it2);
        let result = work.gted(it1, it2);
        self.counter = work.counter;
        self.delta = work.delta;
        result
    }

    /// Number of subproblems encountered in the last distance computation.
    pub fn counter(&self) -> u64 {
        self.counter
    }

    /// Computes the edit mapping between the two input trees. The distance
    /// must be computed first (distances of subtree pairs are required).
    ///
    /// Returns pairs of postorder ids (starting with 1) of mapped nodes.
    /// Deleted and inserted nodes are mapped to 0.
    pub fn compute_edit_mapping(&self) -> Vec<[i32; 2]> {
        let it1 = self
            .it1
            .as_ref()
            .expect("compute the distance before the mapping");
        let it2 = self
            .it2
            .as_ref()
            .expect("compute the distance before the mapping");
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
                if row > first_row
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
        edit_mapping
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
pub fn compute_opt_strategy_post_l<D>(
    it1: &NodeIndexer<'_, D>,
    it2: &NodeIndexer<'_, D>,
) -> Vec<Vec<f32>> {
    let size1 = it1.size() as usize;
    let size2 = it2.size() as usize;
    let mut strategy = vec![vec![0.0f32; size2]; size1];
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
                let mut tmp_cost =
                    size_v as f32 * it2.pre_l_to_kr_sum[wp] as f32 + pool.l[row_v][w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = left_path_v;
                }
                tmp_cost = size_v as f32 * it2.pre_l_to_rev_kr_sum[wp] as f32 + pool.r[row_v][w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = right_path_v;
                }
                tmp_cost = size_v as f32 * it2.pre_l_to_desc_sum[wp] as f32 + pool.i[row_v][w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = strategy[vp][wp] as i32 + 1;
                }
                tmp_cost = size_w as f32 * kr_sum_v as f32 + cost2_l[w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = -(it2.pre_r_to_pre_l
                        [(it2.pre_l_to_pre_r[wp] + size_w - 1) as usize]
                        + path_id_offset
                        + 1);
                }
                tmp_cost = size_w as f32 * revkr_sum_v as f32 + cost2_r[w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = w_in_pre_l + size_w - 1 + path_id_offset + 1;
                }
                tmp_cost = size_w as f32 * desc_sum_v as f32 + cost2_i[w];
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
pub fn compute_opt_strategy_post_r<D>(
    it1: &NodeIndexer<'_, D>,
    it2: &NodeIndexer<'_, D>,
) -> Vec<Vec<f32>> {
    let size1 = it1.size() as usize;
    let size2 = it2.size() as usize;
    let mut strategy = vec![vec![0.0f32; size2]; size1];
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
                let mut tmp_cost = size_v as f32 * it2.pre_l_to_kr_sum[w] as f32 + pool.l[row_v][w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = left_path_v;
                }
                tmp_cost = size_v as f32 * it2.pre_l_to_rev_kr_sum[w] as f32 + pool.r[row_v][w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = right_path_v;
                }
                tmp_cost = size_v as f32 * it2.pre_l_to_desc_sum[w] as f32 + pool.i[row_v][w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = strategy[v][w] as i32 + 1;
                }
                tmp_cost = size_w as f32 * kr_sum_v as f32 + cost2_l[w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = -(it2.pre_r_to_pre_l
                        [(it2.pre_l_to_pre_r[w] + size_w - 1) as usize]
                        + path_id_offset
                        + 1);
                }
                tmp_cost = size_w as f32 * revkr_sum_v as f32 + cost2_r[w];
                if tmp_cost < min_cost {
                    min_cost = tmp_cost;
                    strategy_path = wi + size_w - 1 + path_id_offset + 1;
                }
                tmp_cost = size_w as f32 * desc_sum_v as f32 + cost2_i[w];
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
    fn gted<D>(&mut self, it1: &NodeIndexer<'_, D>, it2: &NodeIndexer<'_, D>) -> f32
    where
        C: CostModel<D>,
    {
        let current_subtree1 = it1.current_node();
        let current_subtree2 = it2.current_node();
        let subtree_size1 = it1.sizes[current_subtree1 as usize];
        let subtree_size2 = it2.sizes[current_subtree2 as usize];

        if subtree_size1 == 1 || subtree_size2 == 1 {
            return self.spf1(it1, current_subtree1, it2, current_subtree2);
        }

        let strategy_path_id =
            self.delta[current_subtree1 as usize][current_subtree2 as usize] as i32;
        let mut current_path_node = strategy_path_id.abs() - 1;
        let path_id_offset = it1.size();

        if current_path_node < path_id_offset {
            let strategy_path_type = get_strategy_path_type(
                strategy_path_id,
                path_id_offset,
                current_subtree1,
                subtree_size1,
            );
            loop {
                let parent = it1.parents[current_path_node as usize];
                if parent < current_subtree1 {
                    break;
                }
                for &child in &it1.children[parent as usize] {
                    if child != current_path_node {
                        it1.set_current_node(child);
                        self.gted(it1, it2);
                    }
                }
                current_path_node = parent;
            }
            it1.set_current_node(current_subtree1);

            // The flag says whether the input subtrees were swapped compared
            // to the original input order [1, Section 3.4].
            if strategy_path_type == LEFT {
                return self.spf_l(it1, it2, false);
            }
            if strategy_path_type == RIGHT {
                return self.spf_r(it1, it2, false);
            }
            return self.spf_a(
                it1,
                it2,
                strategy_path_id.abs() - 1,
                strategy_path_type,
                false,
            );
        }

        current_path_node -= path_id_offset;
        let strategy_path_type = get_strategy_path_type(
            strategy_path_id,
            path_id_offset,
            current_subtree2,
            subtree_size2,
        );
        loop {
            let parent = it2.parents[current_path_node as usize];
            if parent < current_subtree2 {
                break;
            }
            for &child in &it2.children[parent as usize] {
                if child != current_path_node {
                    it2.set_current_node(child);
                    self.gted(it1, it2);
                }
            }
            current_path_node = parent;
        }
        it2.set_current_node(current_subtree2);

        if strategy_path_type == LEFT {
            return self.spf_l(it2, it1, true);
        }
        if strategy_path_type == RIGHT {
            return self.spf_r(it2, it1, true);
        }
        self.spf_a(
            it2,
            it1,
            strategy_path_id.abs() - path_id_offset - 1,
            strategy_path_type,
            true,
        )
    }

    /// Reads `delta` for node `f` of the left-hand tree and node `g` of the
    /// right-hand tree, accounting for swapped input order.
    #[inline]
    fn delta_at(&self, f: i32, g: i32, trees_swapped: bool) -> f32 {
        if trees_swapped {
            self.delta[g as usize][f as usize]
        } else {
            self.delta[f as usize][g as usize]
        }
    }

    /// Writes `delta` for node `f` of the left-hand tree and node `g` of the
    /// right-hand tree, accounting for swapped input order.
    #[inline]
    fn set_delta(&mut self, f: i32, g: i32, trees_swapped: bool, value: f32) {
        if trees_swapped {
            self.delta[g as usize][f as usize] = value;
        } else {
            self.delta[f as usize][g as usize] = value;
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
        let mut current_forest_size2: i32;
        let mut tmp_forest_size1: i32;
        let mut current_forest_cost1: f32 = 0.0;
        let mut current_forest_cost2: f32;
        let mut tmp_forest_cost1: f32;

        let subtree_size2 = it2.sizes[current_subtree_pre_l2 as usize];
        let subtree_size1 = it1.sizes[current_subtree_pre_l1 as usize];
        let w = (subtree_size2 + 1) as usize; // Row width of both s and t.
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
                let r_f_first;
                let l_f_first;
                if start_path_node == -1 {
                    r_f_first = end_path_node_in_pre_r;
                    l_f_first = end_path_node;
                } else {
                    r_f_first = start_path_node_in_pre_r;
                    l_f_first = start_path_node - 1;
                }
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
                        // G_lGfirst.
                        current_forest_size2 = it2sizes[l_g_first as usize];
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
                        if current_forest_size2 == 1 {
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
                            // Increment size and cost of G forest by node lG.
                            current_forest_size2 += 1;
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
                        // Reset size and cost of G forest to G_lG.
                        current_forest_size2 = it2sizes[lgu];
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
                        if current_forest_size2 == 1 {
                            sp2 = current_forest_cost1;
                        } else {
                            sp2 = self.q[r_f as usize];
                        }
                        let mut r_g = r_g_first;
                        let r_g_first_in_pre_l = it2pre_r_to_pre_l[r_g_first as usize];
                        current_forest_size2 += 1;
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
                            // Increment size and cost of G forest by node rG.
                            current_forest_size2 += 1;
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
        let c2 = it2.current_node();
        let c1 = it1.current_node();
        let mut key_roots = vec![-1i32; it2.sizes[c2 as usize] as usize];
        let path_id = it2.pre_l_to_lld(c2);
        let first_key_root = compute_key_roots(it2, c2, path_id, &mut key_roots, 0);
        let mut forestdist = vec![
            vec![0.0f32; it2.sizes[c2 as usize] as usize + 1];
            it1.sizes[c1 as usize] as usize + 1
        ];
        // In the left-hand subtree only the root is a keyroot, so compute the
        // distance between it and every keyroot of the right-hand subtree.
        for i in (0..first_key_root).rev() {
            self.tree_edit_dist(it1, it2, c1, key_roots[i], &mut forestdist, trees_swapped);
        }
        forestdist[it1.sizes[c1 as usize] as usize][it2.sizes[c2 as usize] as usize]
    }

    /// Core of spfL: fills `forestdist` with distances of subforest pairs.
    fn tree_edit_dist<D>(
        &mut self,
        it1: &NodeIndexer<'_, D>,
        it2: &NodeIndexer<'_, D>,
        it1subtree: i32,
        it2subtree: i32,
        forestdist: &mut [Vec<f32>],
        trees_swapped: bool,
    ) where
        C: CostModel<D>,
    {
        let cm = self.cost_model;
        let del_f = |n: &Node<D>| if trees_swapped { cm.ins(n) } else { cm.del(n) };
        let ins_g = |n: &Node<D>| if trees_swapped { cm.del(n) } else { cm.ins(n) };
        let i = it1.pre_l_to_post_l[it1subtree as usize];
        let j = it2.pre_l_to_post_l[it2subtree as usize];
        // Offsets so that forestdist indices start at 0.
        let ioff = it1.post_l_to_lld[i as usize] - 1;
        let joff = it2.post_l_to_lld[j as usize] - 1;
        forestdist[0][0] = 0.0;
        for i1 in 1..=(i - ioff) {
            forestdist[i1 as usize][0] =
                forestdist[(i1 - 1) as usize][0] + del_f(it1.post_l_to_node(i1 + ioff));
        }
        for j1 in 1..=(j - joff) {
            forestdist[0][j1 as usize] =
                forestdist[0][(j1 - 1) as usize] + ins_g(it2.post_l_to_node(j1 + joff));
        }
        for i1 in 1..=(i - ioff) {
            for j1 in 1..=(j - joff) {
                self.counter += 1;
                let (a, b) = (i1 as usize, j1 as usize);
                let n1 = it1.post_l_to_node(i1 + ioff);
                let n2 = it2.post_l_to_node(j1 + joff);
                let u = if trees_swapped {
                    cm.ren(n2, n1)
                } else {
                    cm.ren(n1, n2)
                };
                let da = forestdist[a - 1][b] + del_f(n1);
                let db = forestdist[a][b - 1] + ins_g(n2);
                let dc;
                let f = it1.post_l_to_pre_l[(i1 + ioff) as usize];
                let g = it2.post_l_to_pre_l[(j1 + joff) as usize];
                if it1.post_l_to_lld[(i1 + ioff) as usize] == it1.post_l_to_lld[i as usize]
                    && it2.post_l_to_lld[(j1 + joff) as usize] == it2.post_l_to_lld[j as usize]
                {
                    // Both subforests are subtrees.
                    dc = forestdist[a - 1][b - 1] + u;
                    self.set_delta(f, g, trees_swapped, forestdist[a - 1][b - 1]);
                } else {
                    dc = forestdist[(it1.post_l_to_lld[(i1 + ioff) as usize] - 1 - ioff) as usize]
                        [(it2.post_l_to_lld[(j1 + joff) as usize] - 1 - joff) as usize]
                        + self.delta_at(f, g, trees_swapped)
                        + u;
                }
                forestdist[a][b] = if da >= db {
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
            }
        }
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
        let c2 = it2.current_node();
        let c1 = it1.current_node();
        let mut rev_key_roots = vec![-1i32; it2.sizes[c2 as usize] as usize];
        let path_id = it2.pre_l_to_rld(c2);
        let first_key_root = compute_rev_key_roots(it2, c2, path_id, &mut rev_key_roots, 0);
        let mut forestdist = vec![
            vec![0.0f32; it2.sizes[c2 as usize] as usize + 1];
            it1.sizes[c1 as usize] as usize + 1
        ];
        for i in (0..first_key_root).rev() {
            self.rev_tree_edit_dist(
                it1,
                it2,
                c1,
                rev_key_roots[i],
                &mut forestdist,
                trees_swapped,
            );
        }
        forestdist[it1.sizes[c1 as usize] as usize][it2.sizes[c2 as usize] as usize]
    }

    /// Core of spfR: fills `forestdist` with distances of subforest pairs.
    fn rev_tree_edit_dist<D>(
        &mut self,
        it1: &NodeIndexer<'_, D>,
        it2: &NodeIndexer<'_, D>,
        it1subtree: i32,
        it2subtree: i32,
        forestdist: &mut [Vec<f32>],
        trees_swapped: bool,
    ) where
        C: CostModel<D>,
    {
        let cm = self.cost_model;
        let del_f = |n: &Node<D>| if trees_swapped { cm.ins(n) } else { cm.del(n) };
        let ins_g = |n: &Node<D>| if trees_swapped { cm.del(n) } else { cm.ins(n) };
        let i = it1.pre_l_to_post_r[it1subtree as usize];
        let j = it2.pre_l_to_post_r[it2subtree as usize];
        let ioff = it1.post_r_to_rld[i as usize] - 1;
        let joff = it2.post_r_to_rld[j as usize] - 1;
        forestdist[0][0] = 0.0;
        for i1 in 1..=(i - ioff) {
            forestdist[i1 as usize][0] =
                forestdist[(i1 - 1) as usize][0] + del_f(it1.post_r_to_node(i1 + ioff));
        }
        for j1 in 1..=(j - joff) {
            forestdist[0][j1 as usize] =
                forestdist[0][(j1 - 1) as usize] + ins_g(it2.post_r_to_node(j1 + joff));
        }
        for i1 in 1..=(i - ioff) {
            for j1 in 1..=(j - joff) {
                self.counter += 1;
                let (a, b) = (i1 as usize, j1 as usize);
                let n1 = it1.post_r_to_node(i1 + ioff);
                let n2 = it2.post_r_to_node(j1 + joff);
                let u = if trees_swapped {
                    cm.ren(n2, n1)
                } else {
                    cm.ren(n1, n2)
                };
                let da = forestdist[a - 1][b] + del_f(n1);
                let db = forestdist[a][b - 1] + ins_g(n2);
                let dc;
                let f = it1.post_r_to_pre_l[(i1 + ioff) as usize];
                let g = it2.post_r_to_pre_l[(j1 + joff) as usize];
                if it1.post_r_to_rld[(i1 + ioff) as usize] == it1.post_r_to_rld[i as usize]
                    && it2.post_r_to_rld[(j1 + joff) as usize] == it2.post_r_to_rld[j as usize]
                {
                    dc = forestdist[a - 1][b - 1] + u;
                    self.set_delta(f, g, trees_swapped, forestdist[a - 1][b - 1]);
                } else {
                    dc = forestdist[(it1.post_r_to_rld[(i1 + ioff) as usize] - 1 - ioff) as usize]
                        [(it2.post_r_to_rld[(j1 + joff) as usize] - 1 - joff) as usize]
                        + self.delta_at(f, g, trees_swapped)
                        + u;
                }
                forestdist[a][b] = if da >= db {
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
            }
        }
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

/// Stores the keyroot nodes for left paths of the subtree rooted at
/// `subtree_root_node` into `key_roots`, starting at `index`. Returns the
/// index after the last stored keyroot.
fn compute_key_roots<D>(
    it2: &NodeIndexer<'_, D>,
    subtree_root_node: i32,
    path_id: i32,
    key_roots: &mut [i32],
    mut index: usize,
) -> usize {
    key_roots[index] = subtree_root_node;
    index += 1;
    let mut path_node = path_id;
    while path_node > subtree_root_node {
        let parent = it2.parents[path_node as usize];
        // Every sibling of a path node is a keyroot.
        for &child in &it2.children[parent as usize] {
            if child != path_node {
                index = compute_key_roots(it2, child, it2.pre_l_to_lld(child), key_roots, index);
            }
        }
        path_node = parent;
    }
    index
}

/// Right-path counterpart of [`compute_key_roots`].
fn compute_rev_key_roots<D>(
    it2: &NodeIndexer<'_, D>,
    subtree_root_node: i32,
    path_id: i32,
    rev_key_roots: &mut [i32],
    mut index: usize,
) -> usize {
    rev_key_roots[index] = subtree_root_node;
    index += 1;
    let mut path_node = path_id;
    while path_node > subtree_root_node {
        let parent = it2.parents[path_node as usize];
        for &child in &it2.children[parent as usize] {
            if child != path_node {
                index = compute_rev_key_roots(
                    it2,
                    child,
                    it2.pre_l_to_rld(child),
                    rev_key_roots,
                    index,
                );
            }
        }
        path_node = parent;
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
