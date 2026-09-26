use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SplitDirection {
    /// Side-by-side (left & right)
    Vertical,
    /// Stacked (top & bottom)
    Horizontal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SplitNode {
    Leaf {
        pane_id: String,
    },
    Branch {
        direction: SplitDirection,
        ratio: f32, // between 0.1 and 0.9, defaults to 0.5
        first: Box<SplitNode>,
        second: Box<SplitNode>,
    },
}

impl SplitNode {
    pub fn leaf(pane_id: String) -> Self {
        SplitNode::Leaf { pane_id }
    }

    pub fn split(self, target_pane: &str, new_pane_id: String, direction: SplitDirection) -> Self {
        match self {
            SplitNode::Leaf { pane_id } => {
                if pane_id == target_pane {
                    SplitNode::Branch {
                        direction,
                        ratio: 0.5,
                        first: Box::new(SplitNode::Leaf { pane_id }),
                        second: Box::new(SplitNode::Leaf {
                            pane_id: new_pane_id,
                        }),
                    }
                } else {
                    SplitNode::Leaf { pane_id }
                }
            }
            SplitNode::Branch {
                direction: d,
                ratio,
                first,
                second,
            } => SplitNode::Branch {
                direction: d,
                ratio,
                first: Box::new(first.split(target_pane, new_pane_id.clone(), direction)),
                second: Box::new(second.split(target_pane, new_pane_id, direction)),
            },
        }
    }

    pub fn remove(self, target_pane: &str) -> Option<Self> {
        match self {
            SplitNode::Leaf { pane_id } => {
                if pane_id == target_pane {
                    None
                } else {
                    Some(SplitNode::Leaf { pane_id })
                }
            }
            SplitNode::Branch {
                direction,
                ratio,
                first,
                second,
            } => {
                let new_first = first.remove(target_pane);
                let new_second = second.remove(target_pane);

                match (new_first, new_second) {
                    (Some(f), Some(s)) => Some(SplitNode::Branch {
                        direction,
                        ratio,
                        first: Box::new(f),
                        second: Box::new(s),
                    }),
                    (Some(f), None) => Some(f),
                    (None, Some(s)) => Some(s),
                    (None, None) => None,
                }
            }
        }
    }

    pub fn collect_panes(&self, list: &mut Vec<String>) {
        match self {
            SplitNode::Leaf { pane_id } => list.push(pane_id.clone()),
            SplitNode::Branch { first, second, .. } => {
                first.collect_panes(list);
                second.collect_panes(list);
            }
        }
    }

    pub fn set_ratio_at_path(&mut self, path: &[usize], new_ratio: f32) -> bool {
        let clamped = new_ratio.clamp(0.1, 0.9);
        if path.is_empty() {
            if let SplitNode::Branch { ratio, .. } = self {
                *ratio = clamped;
                return true;
            }
            return false;
        }
        match self {
            SplitNode::Leaf { .. } => false,
            SplitNode::Branch { first, second, .. } => {
                if path[0] == 0 {
                    first.set_ratio_at_path(&path[1..], clamped)
                } else {
                    second.set_ratio_at_path(&path[1..], clamped)
                }
            }
        }
    }

    pub fn swap_panes(&mut self, pane_a: &str, pane_b: &str) -> bool {
        if pane_a == pane_b {
            return false;
        }
        let mut found_a = false;
        let mut found_b = false;
        self.swap_panes_internal(pane_a, pane_b, &mut found_a, &mut found_b);
        found_a && found_b
    }

    fn swap_panes_internal(
        &mut self,
        pane_a: &str,
        pane_b: &str,
        found_a: &mut bool,
        found_b: &mut bool,
    ) {
        match self {
            SplitNode::Leaf { pane_id } => {
                if pane_id == pane_a {
                    *pane_id = pane_b.to_string();
                    *found_a = true;
                } else if pane_id == pane_b {
                    *pane_id = pane_a.to_string();
                    *found_b = true;
                }
            }
            SplitNode::Branch { first, second, .. } => {
                first.swap_panes_internal(pane_a, pane_b, found_a, found_b);
                second.swap_panes_internal(pane_a, pane_b, found_a, found_b);
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SplitLayoutManager {
    pub root: SplitNode,
    pub active_pane_id: String,
    pane_counter: usize,
}

impl SplitLayoutManager {
    pub fn new(initial_pane_id: String) -> Self {
        Self {
            root: SplitNode::leaf(initial_pane_id.clone()),
            active_pane_id: initial_pane_id,
            pane_counter: 1,
        }
    }

    pub fn generate_next_pane_id(&mut self) -> String {
        self.pane_counter += 1;
        format!("pane_{}", self.pane_counter)
    }

    pub fn split_active(&mut self, direction: SplitDirection) -> String {
        let new_pane = self.generate_next_pane_id();
        let target = self.active_pane_id.clone();
        let old_root =
            std::mem::replace(&mut self.root, SplitNode::leaf("placeholder".to_string()));
        self.root = old_root.split(&target, new_pane.clone(), direction);
        self.active_pane_id = new_pane.clone();
        new_pane
    }

    pub fn close_pane(&mut self, pane_id: &str) -> bool {
        let all = self.panes();
        if all.len() <= 1 {
            return false; // Cannot close the last pane
        }

        let old_root =
            std::mem::replace(&mut self.root, SplitNode::leaf("placeholder".to_string()));
        if let Some(new_root) = old_root.remove(pane_id) {
            self.root = new_root;
            if self.active_pane_id == pane_id {
                let remaining = self.panes();
                if let Some(first) = remaining.first() {
                    self.active_pane_id = first.clone();
                }
            }
            true
        } else {
            false
        }
    }

    pub fn panes(&self) -> Vec<String> {
        let mut list = Vec::new();
        self.root.collect_panes(&mut list);
        list
    }

    pub fn set_active(&mut self, pane_id: String) {
        if self.panes().contains(&pane_id) {
            self.active_pane_id = pane_id;
        }
    }

    pub fn set_ratio_at_path(&mut self, path: &[usize], new_ratio: f32) -> bool {
        self.root.set_ratio_at_path(path, new_ratio)
    }

    pub fn swap_panes(&mut self, pane_a: &str, pane_b: &str) -> bool {
        self.root.swap_panes(pane_a, pane_b)
    }

    pub fn move_pane_forward(&mut self, pane_id: &str) -> bool {
        let panes = self.panes();
        if let Some(idx) = panes.iter().position(|p| p == pane_id) {
            (idx + 1 < panes.len()) && self.swap_panes(pane_id, &panes[idx + 1])
        } else {
            false
        }
    }

    pub fn move_pane_backward(&mut self, pane_id: &str) -> bool {
        let panes = self.panes();
        if let Some(idx) = panes.iter().position(|p| p == pane_id) {
            (idx > 0) && self.swap_panes(pane_id, &panes[idx - 1])
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_and_close() {
        let mut mgr = SplitLayoutManager::new("pane_1".to_string());
        assert_eq!(mgr.panes(), vec!["pane_1"]);

        // Split vertical
        let pane_2 = mgr.split_active(SplitDirection::Vertical);
        assert_eq!(pane_2, "pane_2");
        assert_eq!(mgr.panes(), vec!["pane_1", "pane_2"]);
        assert_eq!(mgr.active_pane_id, "pane_2");

        // Split horizontal on pane 2
        let pane_3 = mgr.split_active(SplitDirection::Horizontal);
        assert_eq!(pane_3, "pane_3");
        assert_eq!(mgr.panes(), vec!["pane_1", "pane_2", "pane_3"]);

        // Close pane 2
        let closed = mgr.close_pane("pane_2");
        assert!(closed);
        assert_eq!(mgr.panes(), vec!["pane_1", "pane_3"]);

        // Try close last panes until 1
        mgr.close_pane("pane_3");
        assert_eq!(mgr.panes(), vec!["pane_1"]);

        // Closing the only remaining pane must fail
        assert!(!mgr.close_pane("pane_1"));
        assert_eq!(mgr.panes(), vec!["pane_1"]);
    }

    #[test]
    fn test_split_ratio_and_swapping() {
        let mut mgr = SplitLayoutManager::new("pane_1".to_string());
        let _pane_2 = mgr.split_active(SplitDirection::Vertical);

        // Test ratio update at root
        assert!(mgr.set_ratio_at_path(&[], 0.7));
        if let SplitNode::Branch { ratio, .. } = &mgr.root {
            assert!((ratio - 0.7).abs() < 1e-4);
        } else {
            panic!("Expected branch at root");
        }

        // Test clamp bounds (0.1 to 0.9)
        assert!(mgr.set_ratio_at_path(&[], 0.05));
        if let SplitNode::Branch { ratio, .. } = &mgr.root {
            assert!((ratio - 0.1).abs() < 1e-4);
        }

        assert!(mgr.set_ratio_at_path(&[], 0.95));
        if let SplitNode::Branch { ratio, .. } = &mgr.root {
            assert!((ratio - 0.9).abs() < 1e-4);
        }

        // Test swapping panes
        assert_eq!(mgr.panes(), vec!["pane_1", "pane_2"]);
        assert!(mgr.swap_panes("pane_1", "pane_2"));
        assert_eq!(mgr.panes(), vec!["pane_2", "pane_1"]);

        // Test move forward / backward
        assert!(mgr.move_pane_backward("pane_1"));
        assert_eq!(mgr.panes(), vec!["pane_1", "pane_2"]);

        assert!(mgr.move_pane_forward("pane_1"));
        assert_eq!(mgr.panes(), vec!["pane_2", "pane_1"]);
    }
}
