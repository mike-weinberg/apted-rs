//! Port of `node.NodeIndexer`: indexes the nodes of an input tree and stores
//! the auxiliary arrays APTED needs. Node ids are `i32` so that `-1` can mark
//! "no node", as in the Java implementation.

use std::cell::Cell;

use super::Node;
use crate::cost_model::CostModel;

/// Indexes the nodes of an input tree in several traversal orders.
///
/// Naming follows the Java implementation: `pre_l` is left-to-right
/// preorder, `pre_r` right-to-left preorder, `post_l` left-to-right
/// postorder, `post_r` right-to-left postorder, `lld` leftmost leaf
/// descendant, `rld` rightmost leaf descendant, `ln` previous leaf node.
pub(crate) struct NodeIndexer<'a, D> {
    /// Nodes by left-to-right preorder id.
    pub(crate) pre_l_to_node: Vec<&'a Node<D>>,
    /// Subtree sizes by left-to-right preorder id.
    pub(crate) sizes: Vec<i32>,
    /// Parent preorder ids (`-1` for the root).
    pub(crate) parents: Vec<i32>,
    /// Children preorder ids, in left-to-right order.
    pub(crate) children: Vec<Vec<i32>>,
    pub(crate) post_l_to_lld: Vec<i32>,
    pub(crate) post_r_to_rld: Vec<i32>,
    pub(crate) pre_l_to_ln: Vec<i32>,
    pub(crate) pre_r_to_ln: Vec<i32>,
    /// True for nodes that are the leftmost child of their parent.
    pub(crate) node_type_l: Vec<bool>,
    /// True for nodes that are the rightmost child of their parent.
    pub(crate) node_type_r: Vec<bool>,
    pub(crate) pre_l_to_pre_r: Vec<i32>,
    pub(crate) pre_r_to_pre_l: Vec<i32>,
    pub(crate) pre_l_to_post_l: Vec<i32>,
    pub(crate) post_l_to_pre_l: Vec<i32>,
    pub(crate) pre_l_to_post_r: Vec<i32>,
    pub(crate) post_r_to_pre_l: Vec<i32>,
    /// Cost of the spf_L single-path function per subtree. The three cost
    /// arrays are summed in `i64` and stored as `f32`, the type the strategy
    /// computation uses; `i32` sums (as in Java) overflow above ~46k nodes.
    pub(crate) pre_l_to_kr_sum: Vec<f32>,
    /// Cost of the spf_R single-path function per subtree.
    pub(crate) pre_l_to_rev_kr_sum: Vec<f32>,
    /// Cost of the spf_A single-path function per subtree.
    pub(crate) pre_l_to_desc_sum: Vec<f32>,
    /// Cost of deleting every node of the subtree.
    pub(crate) pre_l_to_sum_del_cost: Vec<f32>,
    /// Cost of inserting every node of the subtree.
    pub(crate) pre_l_to_sum_ins_cost: Vec<f32>,
    /// Number of leftmost-child leaves.
    pub(crate) lchl: i32,
    /// Number of rightmost-child leaves.
    pub(crate) rchl: i32,
    current_node: Cell<i32>,
    tree_size: i32,
}

/// A node of `index_nodes`' explicit traversal stack, with the sums
/// accumulated over the children finished so far.
struct Frame<'a, D> {
    node: &'a Node<D>,
    preorder: i32,
    next_child: usize,
    children_preorders: Vec<i32>,
    current_size: i64,
    desc_sizes: i64,
    kr_sizes_sum: i64,
    revkr_sizes_sum: i64,
}

impl<'a, D> Frame<'a, D> {
    fn new(node: &'a Node<D>, preorder: i32) -> Self {
        Frame {
            node,
            preorder,
            next_child: 0,
            children_preorders: Vec::with_capacity(node.children().len()),
            current_size: 0,
            desc_sizes: 0,
            kr_sizes_sum: 0,
            revkr_sizes_sum: 0,
        }
    }
}

impl<'a, D> NodeIndexer<'a, D> {
    pub(crate) fn new<C: CostModel<D> + ?Sized>(input_tree: &'a Node<D>, cost_model: &C) -> Self {
        let n = input_tree.node_count();
        let mut ni = NodeIndexer {
            pre_l_to_node: Vec::with_capacity(n),
            sizes: vec![0; n],
            parents: vec![0; n],
            children: vec![Vec::new(); n],
            post_l_to_lld: vec![0; n],
            post_r_to_rld: vec![0; n],
            pre_l_to_ln: vec![0; n],
            pre_r_to_ln: vec![0; n],
            node_type_l: vec![false; n],
            node_type_r: vec![false; n],
            pre_l_to_pre_r: vec![0; n],
            pre_r_to_pre_l: vec![0; n],
            pre_l_to_post_l: vec![0; n],
            post_l_to_pre_l: vec![0; n],
            pre_l_to_post_r: vec![0; n],
            post_r_to_pre_l: vec![0; n],
            pre_l_to_kr_sum: vec![0.0; n],
            pre_l_to_rev_kr_sum: vec![0.0; n],
            pre_l_to_desc_sum: vec![0.0; n],
            pre_l_to_sum_del_cost: vec![0.0; n],
            pre_l_to_sum_ins_cost: vec![0.0; n],
            lchl: 0,
            rchl: 0,
            current_node: Cell::new(0),
            tree_size: n as i32,
        };
        ni.parents[0] = -1; // The root has no parent.
        let mut nodes: Vec<Option<&'a Node<D>>> = vec![None; n];
        ni.index_nodes(input_tree, &mut nodes);
        ni.pre_l_to_node = nodes
            .into_iter()
            .map(|x| x.expect("every node indexed"))
            .collect();
        ni.post_traversal_indexing(cost_model);
        ni
    }

    /// Indexes every node in one depth-first traversal. Iterative (an
    /// explicit stack of [`Frame`]s replaces the Java recursion), so any tree
    /// depth is safe; the arithmetic matches the recursive original.
    fn index_nodes(&mut self, root: &'a Node<D>, nodes: &mut [Option<&'a Node<D>>]) {
        let mut postorder: i32 = -1;
        let mut next_preorder: i32 = 1;
        let mut stack = vec![Frame::new(root, 0)];
        while let Some(top) = stack.last_mut() {
            if let Some(child) = top.node.children().get(top.next_child) {
                top.next_child += 1;
                let child_preorder = next_preorder;
                next_preorder += 1;
                self.parents[child_preorder as usize] = top.preorder;
                top.children_preorders.push(child_preorder);
                stack.push(Frame::new(child, child_preorder));
                continue;
            }

            // All children done: finish this node.
            let f = stack.pop().expect("stack is non-empty");
            postorder += 1;
            let preorder = f.preorder;
            let p = preorder as usize;
            let current_size = f.current_size;
            let current_desc_sizes = f.desc_sizes + current_size + 1;
            self.pre_l_to_desc_sum[p] =
                (((current_size + 1) * (current_size + 1 + 3)) / 2 - current_desc_sizes) as f32;
            self.pre_l_to_kr_sum[p] = (f.kr_sizes_sum + current_size + 1) as f32;
            self.pre_l_to_rev_kr_sum[p] = (f.revkr_sizes_sum + current_size + 1) as f32;
            nodes[p] = Some(f.node);
            self.sizes[p] = (current_size + 1) as i32;
            let preorder_r = self.tree_size - 1 - postorder;
            self.pre_l_to_pre_r[p] = preorder_r;
            self.pre_r_to_pre_l[preorder_r as usize] = preorder;
            self.children[p] = f.children_preorders;
            self.post_l_to_pre_l[postorder as usize] = preorder;
            self.pre_l_to_post_l[p] = postorder;
            self.pre_l_to_post_r[p] = self.tree_size - 1 - preorder;
            self.post_r_to_pre_l[(self.tree_size - 1 - preorder) as usize] = preorder;

            // Add this subtree's sums to its parent.
            if let Some(parent) = stack.last_mut() {
                let idx = parent.next_child - 1; // position among siblings
                parent.current_size += 1 + current_size;
                parent.desc_sizes += current_desc_sizes;
                if idx > 0 {
                    parent.kr_sizes_sum += f.kr_sizes_sum + current_size + 1;
                } else {
                    parent.kr_sizes_sum += f.kr_sizes_sum;
                    self.node_type_l[p] = true;
                }
                if idx + 1 < parent.node.children().len() {
                    parent.revkr_sizes_sum += f.revkr_sizes_sum + current_size + 1;
                } else {
                    parent.revkr_sizes_sum += f.revkr_sizes_sum;
                    self.node_type_r[p] = true;
                }
            }
        }
    }

    /// Computes the arrays that need the traversal orders built by
    /// `index_nodes`.
    fn post_traversal_indexing<C: CostModel<D> + ?Sized>(&mut self, cost_model: &C) {
        let mut current_leaf = -1;
        for i in 0..self.tree_size {
            let iu = i as usize;
            self.pre_l_to_ln[iu] = current_leaf;
            if self.is_leaf(i) {
                current_leaf = i;
            }

            // Left-to-right postorder iteration.
            let postl = iu;
            let preorder = self.post_l_to_pre_l[iu] as usize;
            self.post_l_to_lld[postl] = if self.sizes[preorder] == 1 {
                postl as i32
            } else {
                self.post_l_to_lld
                    [self.pre_l_to_post_l[self.children[preorder][0] as usize] as usize]
            };

            // Right-to-left postorder iteration.
            let postr = iu;
            let preorder = self.post_r_to_pre_l[postr] as usize;
            self.post_r_to_rld[postr] = if self.sizes[preorder] == 1 {
                postr as i32
            } else {
                let last = *self.children[preorder]
                    .last()
                    .expect("inner node has children");
                self.post_r_to_rld[self.pre_l_to_post_r[last as usize] as usize]
            };

            // Count left-child and right-child leaves.
            if self.sizes[iu] == 1 {
                let parent = self.parents[iu];
                if parent > -1 {
                    if parent + 1 == i {
                        self.lchl += 1;
                    } else if self.pre_l_to_pre_r[parent as usize] + 1 == self.pre_l_to_pre_r[iu] {
                        self.rchl += 1;
                    }
                }
            }

            // Sum up subtree deletion and insertion costs.
            let node_for_sum = (self.tree_size - i - 1) as usize;
            let parent_for_sum = self.parents[node_for_sum];
            self.pre_l_to_sum_del_cost[node_for_sum] +=
                cost_model.del(self.pre_l_to_node[node_for_sum]);
            self.pre_l_to_sum_ins_cost[node_for_sum] +=
                cost_model.ins(self.pre_l_to_node[node_for_sum]);
            if parent_for_sum > -1 {
                let pf = parent_for_sum as usize;
                self.pre_l_to_sum_del_cost[pf] += self.pre_l_to_sum_del_cost[node_for_sum];
                self.pre_l_to_sum_ins_cost[pf] += self.pre_l_to_sum_ins_cost[node_for_sum];
            }
        }

        current_leaf = -1;
        for i in 0..self.sizes[0] {
            self.pre_r_to_ln[i as usize] = current_leaf;
            if self.is_leaf(self.pre_r_to_pre_l[i as usize]) {
                current_leaf = i;
            }
        }
    }

    /// Leftmost leaf descendant of a node, both in left-to-right preorder.
    pub(crate) fn pre_l_to_lld(&self, pre_l: i32) -> i32 {
        self.post_l_to_pre_l
            [self.post_l_to_lld[self.pre_l_to_post_l[pre_l as usize] as usize] as usize]
    }

    /// Rightmost leaf descendant of a node, both in left-to-right preorder.
    pub(crate) fn pre_l_to_rld(&self, pre_l: i32) -> i32 {
        self.post_r_to_pre_l
            [self.post_r_to_rld[self.pre_l_to_post_r[pre_l as usize] as usize] as usize]
    }

    pub(crate) fn post_l_to_node(&self, post_l: i32) -> &'a Node<D> {
        self.pre_l_to_node[self.post_l_to_pre_l[post_l as usize] as usize]
    }

    pub(crate) fn size(&self) -> i32 {
        self.tree_size
    }

    pub(crate) fn is_leaf(&self, node: i32) -> bool {
        self.sizes[node as usize] == 1
    }

    /// Root of the subtree currently processed in the tree decomposition.
    pub(crate) fn current_node(&self) -> i32 {
        self.current_node.get()
    }

    pub(crate) fn set_current_node(&self, preorder: i32) {
        self.current_node.set(preorder);
    }
}
