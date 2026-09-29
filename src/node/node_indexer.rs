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
pub struct NodeIndexer<'a, D> {
    /// Nodes by left-to-right preorder id.
    pub pre_l_to_node: Vec<&'a Node<D>>,
    /// Subtree sizes by left-to-right preorder id.
    pub sizes: Vec<i32>,
    /// Parent preorder ids (`-1` for the root).
    pub parents: Vec<i32>,
    /// Children preorder ids, in left-to-right order.
    pub children: Vec<Vec<i32>>,
    pub post_l_to_lld: Vec<i32>,
    pub post_r_to_rld: Vec<i32>,
    pub pre_l_to_ln: Vec<i32>,
    pub pre_r_to_ln: Vec<i32>,
    /// True for nodes that are the leftmost child of their parent.
    pub node_type_l: Vec<bool>,
    /// True for nodes that are the rightmost child of their parent.
    pub node_type_r: Vec<bool>,
    pub pre_l_to_pre_r: Vec<i32>,
    pub pre_r_to_pre_l: Vec<i32>,
    pub pre_l_to_post_l: Vec<i32>,
    pub post_l_to_pre_l: Vec<i32>,
    pub pre_l_to_post_r: Vec<i32>,
    pub post_r_to_pre_l: Vec<i32>,
    /// Cost of the spf_L single-path function per subtree.
    pub pre_l_to_kr_sum: Vec<i32>,
    /// Cost of the spf_R single-path function per subtree.
    pub pre_l_to_rev_kr_sum: Vec<i32>,
    /// Cost of the spf_A single-path function per subtree.
    pub pre_l_to_desc_sum: Vec<i32>,
    /// Cost of deleting every node of the subtree.
    pub pre_l_to_sum_del_cost: Vec<f32>,
    /// Cost of inserting every node of the subtree.
    pub pre_l_to_sum_ins_cost: Vec<f32>,
    /// Number of leftmost-child leaves.
    pub lchl: i32,
    /// Number of rightmost-child leaves.
    pub rchl: i32,
    current_node: Cell<i32>,
    tree_size: i32,
}

/// Scratch values passed between recursive calls of `index_nodes`.
#[derive(Default)]
struct Tmp {
    size: i32,
    desc_sizes: i32,
    kr_sizes_sum: i32,
    revkr_sizes_sum: i32,
    preorder: i32,
}

impl<'a, D> NodeIndexer<'a, D> {
    pub fn new<C: CostModel<D> + ?Sized>(input_tree: &'a Node<D>, cost_model: &C) -> Self {
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
            pre_l_to_kr_sum: vec![0; n],
            pre_l_to_rev_kr_sum: vec![0; n],
            pre_l_to_desc_sum: vec![0; n],
            pre_l_to_sum_del_cost: vec![0.0; n],
            pre_l_to_sum_ins_cost: vec![0.0; n],
            lchl: 0,
            rchl: 0,
            current_node: Cell::new(0),
            tree_size: n as i32,
        };
        ni.parents[0] = -1; // The root has no parent.
        let mut nodes: Vec<Option<&'a Node<D>>> = vec![None; n];
        let mut tmp = Tmp::default();
        ni.index_nodes(input_tree, -1, &mut tmp, &mut nodes);
        ni.pre_l_to_node = nodes
            .into_iter()
            .map(|x| x.expect("every node indexed"))
            .collect();
        ni.post_traversal_indexing(cost_model);
        ni
    }

    /// Indexes the nodes of the subtree rooted at `node` recursively. Returns
    /// the postorder id of `node`.
    fn index_nodes(
        &mut self,
        node: &'a Node<D>,
        mut postorder: i32,
        tmp: &mut Tmp,
        nodes: &mut [Option<&'a Node<D>>],
    ) -> i32 {
        let mut current_size = 0;
        let mut children_count = 0;
        let mut desc_sizes = 0;
        let mut kr_sizes_sum = 0;
        let mut revkr_sizes_sum = 0;
        let preorder = tmp.preorder;
        let mut children_preorders = Vec::with_capacity(node.children().len());
        tmp.preorder += 1;
        let n_children = node.children().len();
        for (idx, child) in node.children().iter().enumerate() {
            children_count += 1;
            let current_preorder = tmp.preorder;
            self.parents[current_preorder as usize] = preorder;
            postorder = self.index_nodes(child, postorder, tmp, nodes);
            children_preorders.push(current_preorder);
            current_size += 1 + tmp.size;
            desc_sizes += tmp.desc_sizes;
            if children_count > 1 {
                kr_sizes_sum += tmp.kr_sizes_sum + tmp.size + 1;
            } else {
                kr_sizes_sum += tmp.kr_sizes_sum;
                self.node_type_l[current_preorder as usize] = true;
            }
            if idx + 1 < n_children {
                revkr_sizes_sum += tmp.revkr_sizes_sum + tmp.size + 1;
            } else {
                revkr_sizes_sum += tmp.revkr_sizes_sum;
                self.node_type_r[current_preorder as usize] = true;
            }
        }
        postorder += 1;
        let p = preorder as usize;
        let current_desc_sizes = desc_sizes + current_size + 1;
        self.pre_l_to_desc_sum[p] =
            ((current_size + 1) * (current_size + 1 + 3)) / 2 - current_desc_sizes;
        self.pre_l_to_kr_sum[p] = kr_sizes_sum + current_size + 1;
        self.pre_l_to_rev_kr_sum[p] = revkr_sizes_sum + current_size + 1;
        nodes[p] = Some(node);
        self.sizes[p] = current_size + 1;
        let preorder_r = self.tree_size - 1 - postorder;
        self.pre_l_to_pre_r[p] = preorder_r;
        self.pre_r_to_pre_l[preorder_r as usize] = preorder;
        self.children[p] = children_preorders;
        tmp.desc_sizes = current_desc_sizes;
        tmp.size = current_size;
        tmp.kr_sizes_sum = kr_sizes_sum;
        tmp.revkr_sizes_sum = revkr_sizes_sum;
        self.post_l_to_pre_l[postorder as usize] = preorder;
        self.pre_l_to_post_l[p] = postorder;
        self.pre_l_to_post_r[p] = self.tree_size - 1 - preorder;
        self.post_r_to_pre_l[(self.tree_size - 1 - preorder) as usize] = preorder;
        postorder
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
    pub fn pre_l_to_lld(&self, pre_l: i32) -> i32 {
        self.post_l_to_pre_l
            [self.post_l_to_lld[self.pre_l_to_post_l[pre_l as usize] as usize] as usize]
    }

    /// Rightmost leaf descendant of a node, both in left-to-right preorder.
    pub fn pre_l_to_rld(&self, pre_l: i32) -> i32 {
        self.post_r_to_pre_l
            [self.post_r_to_rld[self.pre_l_to_post_r[pre_l as usize] as usize] as usize]
    }

    pub fn post_l_to_node(&self, post_l: i32) -> &'a Node<D> {
        self.pre_l_to_node[self.post_l_to_pre_l[post_l as usize] as usize]
    }

    pub fn post_r_to_node(&self, post_r: i32) -> &'a Node<D> {
        self.pre_l_to_node[self.post_r_to_pre_l[post_r as usize] as usize]
    }

    pub fn size(&self) -> i32 {
        self.tree_size
    }

    pub fn is_leaf(&self, node: i32) -> bool {
        self.sizes[node as usize] == 1
    }

    /// Root of the subtree currently processed in the tree decomposition.
    pub fn current_node(&self) -> i32 {
        self.current_node.get()
    }

    pub fn set_current_node(&self, preorder: i32) {
        self.current_node.set(preorder);
    }
}
