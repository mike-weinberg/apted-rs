//! Port of `distance.AllPossibleMappingsTED`: computes the tree edit distance
//! by enumerating every one-to-one mapping and keeping the valid TED
//! mappings. Exponential; only for testing on very small trees.

use crate::cost_model::CostModel;
use crate::node::{Node, NodeIndexer};

/// A mapping element: a pair of preorder ids, `-1` for deletion/insertion.
type Mapping = Vec<[i32; 2]>;

pub struct AllPossibleMappingsTED<'a, C, D> {
    cost_model: C,
    it1: Option<NodeIndexer<'a, D>>,
    it2: Option<NodeIndexer<'a, D>>,
    size1: i32,
    size2: i32,
}

impl<'a, C: CostModel<D>, D> AllPossibleMappingsTED<'a, C, D> {
    pub fn new(cost_model: C) -> Self {
        Self {
            cost_model,
            it1: None,
            it2: None,
            size1: 0,
            size2: 0,
        }
    }

    pub fn compute_edit_distance(&mut self, t1: &'a Node<D>, t2: &'a Node<D>) -> f32 {
        self.init(t1, t2);
        let mut mappings = self.generate_all_one_to_one_mappings();
        mappings.retain(|m| self.is_ted_mapping(m));
        self.get_min_cost(&mappings)
    }

    pub fn init(&mut self, t1: &'a Node<D>, t2: &'a Node<D>) {
        let it1 = NodeIndexer::new(t1, &self.cost_model);
        let it2 = NodeIndexer::new(t2, &self.cost_model);
        self.size1 = it1.size();
        self.size2 = it2.size();
        self.it1 = Some(it1);
        self.it2 = Some(it2);
    }

    fn generate_all_one_to_one_mappings(&self) -> Vec<Mapping> {
        let mut first: Mapping = Vec::with_capacity((self.size1 + self.size2) as usize);
        for n1 in 0..self.size1 {
            first.push([n1, -1]);
        }
        for n2 in 0..self.size2 {
            first.push([-1, n2]);
        }
        let mut mappings = vec![first];
        for n1 in 0..self.size1 {
            let mappings_copy = mappings.clone();
            for n2 in 0..self.size2 {
                for m in &mappings_copy {
                    let element_add = !m.iter().any(|e| e[0] != -1 && e[1] != -1 && e[1] == n2);
                    if element_add {
                        let mut m_copy = m.clone();
                        m_copy.push([n1, n2]);
                        remove_mapping_element(&mut m_copy, [n1, -1]);
                        remove_mapping_element(&mut m_copy, [-1, n2]);
                        mappings.push(m_copy);
                    }
                }
            }
        }
        mappings
    }

    fn is_ted_mapping(&self, m: &Mapping) -> bool {
        let it1 = self.it1.as_ref().unwrap();
        let it2 = self.it2.as_ref().unwrap();
        let pr1 = |x: i32| it1.pre_l_to_pre_r[x as usize];
        let pr2 = |x: i32| it2.pre_l_to_pre_r[x as usize];
        for e1 in m {
            if e1[0] == -1 || e1[1] == -1 {
                continue;
            }
            for e2 in m {
                if e2[0] == -1 || e2[1] == -1 {
                    continue;
                }
                // Ancestor-descendant relationship must be preserved.
                let a = e1[0] < e2[0] && pr1(e1[0]) < pr1(e2[0]);
                let b = e1[1] < e2[1] && pr2(e1[1]) < pr2(e2[1]);
                if a != b {
                    return false;
                }
                // Left-right (sibling) order must be preserved.
                let a = e1[0] < e2[0] && pr1(e1[0]) > pr1(e2[0]);
                let b = e1[1] < e2[1] && pr2(e1[1]) > pr2(e2[1]);
                if a != b {
                    return false;
                }
            }
        }
        true
    }

    fn get_min_cost(&self, ted_mappings: &[Mapping]) -> f32 {
        let it1 = self.it1.as_ref().unwrap();
        let it2 = self.it2.as_ref().unwrap();
        let cm = &self.cost_model;
        let mut min_cost = (self.size1 + self.size2) as f32;
        for m in ted_mappings {
            let mut m_cost = 0.0f32;
            for e in m {
                if e[0] > -1 && e[1] > -1 {
                    m_cost += cm.ren(
                        it1.pre_l_to_node[e[0] as usize],
                        it2.pre_l_to_node[e[1] as usize],
                    );
                } else if e[0] > -1 {
                    m_cost += cm.del(it1.pre_l_to_node[e[0] as usize]);
                } else {
                    m_cost += cm.ins(it2.pre_l_to_node[e[1] as usize]);
                }
                if m_cost >= min_cost {
                    break;
                }
            }
            if m_cost < min_cost {
                min_cost = m_cost;
            }
        }
        min_cost
    }
}

/// Removes the first occurrence of `e` from `m`.
fn remove_mapping_element(m: &mut Mapping, e: [i32; 2]) -> bool {
    if let Some(pos) = m.iter().position(|me| *me == e) {
        m.remove(pos);
        true
    } else {
        false
    }
}
