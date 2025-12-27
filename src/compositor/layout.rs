//! Tiling layout using Binary Space Partition (BSP)
//!
//! The layout automatically arranges windows in a tiling pattern:
//! - First window fills the screen
//! - Second window splits horizontally (left/right)
//! - Third window splits the second area vertically
//! - Pattern continues recursively
//!
//! ```text
//! ┌───────────────────────────────────────┐
//! │ Terminal                              │
//! ├───────────────────┬───────────────────┤
//! │ Files             │ Editor            │
//! │                   │                   │
//! │                   │                   │
//! └───────────────────┴───────────────────┘
//! ```

use super::geometry::Rect;
use super::window::WindowId;
use std::collections::HashMap;

/// Direction of a split
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    /// Split horizontally (left | right)
    Horizontal,
    /// Split vertically (top / bottom)
    Vertical,
}

impl SplitDirection {
    /// Toggle between horizontal and vertical
    pub fn toggle(&self) -> Self {
        match self {
            Self::Horizontal => Self::Vertical,
            Self::Vertical => Self::Horizontal,
        }
    }
}

/// A node in the BSP tree
#[derive(Debug, Clone)]
pub enum LayoutNode {
    /// A leaf node containing a window
    Window(WindowId),
    /// An internal node with a split
    Split {
        direction: SplitDirection,
        /// Split ratio (0.0 to 1.0, where 0.5 = equal split)
        ratio: f32,
        /// First child (left or top)
        first: Box<LayoutNode>,
        /// Second child (right or bottom)
        second: Box<LayoutNode>,
    },
}

impl LayoutNode {
    /// Create a new window node
    pub fn window(id: WindowId) -> Self {
        Self::Window(id)
    }

    /// Create a split node
    pub fn split(
        direction: SplitDirection,
        ratio: f32,
        first: LayoutNode,
        second: LayoutNode,
    ) -> Self {
        Self::Split {
            direction,
            ratio,
            first: Box::new(first),
            second: Box::new(second),
        }
    }

    /// Check if this node contains a window
    pub fn contains(&self, id: WindowId) -> bool {
        match self {
            Self::Window(wid) => *wid == id,
            Self::Split { first, second, .. } => first.contains(id) || second.contains(id),
        }
    }

    /// Count the number of windows in this subtree
    pub fn window_count(&self) -> usize {
        match self {
            Self::Window(_) => 1,
            Self::Split { first, second, .. } => first.window_count() + second.window_count(),
        }
    }

    /// Get all window IDs in this subtree
    pub fn window_ids(&self) -> Vec<WindowId> {
        match self {
            Self::Window(id) => vec![*id],
            Self::Split { first, second, .. } => {
                let mut ids = first.window_ids();
                ids.extend(second.window_ids());
                ids
            }
        }
    }

    /// Calculate rectangles for all windows given a bounding rect
    pub fn calculate_rects(&self, bounds: Rect) -> HashMap<WindowId, Rect> {
        let mut result = HashMap::new();
        self.calculate_rects_recursive(bounds, &mut result);
        result
    }

    fn calculate_rects_recursive(&self, bounds: Rect, result: &mut HashMap<WindowId, Rect>) {
        match self {
            Self::Window(id) => {
                result.insert(*id, bounds);
            }
            Self::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let (first_bounds, second_bounds) = match direction {
                    SplitDirection::Horizontal => bounds.split_horizontal(*ratio),
                    SplitDirection::Vertical => bounds.split_vertical(*ratio),
                };
                first.calculate_rects_recursive(first_bounds, result);
                second.calculate_rects_recursive(second_bounds, result);
            }
        }
    }

    /// Remove a window from the tree, returning the modified tree (or None if empty)
    pub fn remove(&self, id: WindowId) -> Option<LayoutNode> {
        match self {
            Self::Window(wid) => {
                if *wid == id {
                    None
                } else {
                    Some(self.clone())
                }
            }
            Self::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let first_result = first.remove(id);
                let second_result = second.remove(id);

                match (first_result, second_result) {
                    (None, None) => None,
                    (Some(node), None) | (None, Some(node)) => Some(node),
                    (Some(f), Some(s)) => Some(Self::Split {
                        direction: *direction,
                        ratio: *ratio,
                        first: Box::new(f),
                        second: Box::new(s),
                    }),
                }
            }
        }
    }

    /// Insert a window next to an existing window
    pub fn insert_next_to(
        &self,
        existing: WindowId,
        new_id: WindowId,
        direction: SplitDirection,
    ) -> LayoutNode {
        match self {
            Self::Window(wid) => {
                if *wid == existing {
                    // Split this window
                    Self::Split {
                        direction,
                        ratio: 0.5,
                        first: Box::new(Self::Window(*wid)),
                        second: Box::new(Self::Window(new_id)),
                    }
                } else {
                    self.clone()
                }
            }
            Self::Split {
                direction: dir,
                ratio,
                first,
                second,
            } => Self::Split {
                direction: *dir,
                ratio: *ratio,
                first: Box::new(first.insert_next_to(existing, new_id, direction)),
                second: Box::new(second.insert_next_to(existing, new_id, direction)),
            },
        }
    }
}

/// The tiling layout manager
#[derive(Debug, Clone)]
pub struct TilingLayout {
    /// Root of the BSP tree (None if no windows)
    root: Option<LayoutNode>,
    /// Bounding rectangle for the layout
    bounds: Rect,
    /// Gap between windows in pixels
    gap: f64,
    /// Outer margin in pixels
    margin: f64,
    /// Next split direction (alternates)
    next_direction: SplitDirection,
}

impl TilingLayout {
    /// Create a new tiling layout
    pub fn new(bounds: Rect) -> Self {
        Self {
            root: None,
            bounds,
            gap: 4.0,
            margin: 4.0,
            next_direction: SplitDirection::Horizontal,
        }
    }

    /// Set the bounding rectangle
    pub fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    /// Get the bounding rectangle
    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    /// Set the gap between windows
    pub fn set_gap(&mut self, gap: f64) {
        self.gap = gap;
    }

    /// Set the outer margin
    pub fn set_margin(&mut self, margin: f64) {
        self.margin = margin;
    }

    /// Add a window to the layout
    pub fn add_window(&mut self, id: WindowId) {
        match &self.root {
            None => {
                // First window
                self.root = Some(LayoutNode::Window(id));
            }
            Some(root) => {
                // Find the last window and split next to it
                let window_ids = root.window_ids();
                if let Some(last_id) = window_ids.last() {
                    let new_root = root.insert_next_to(*last_id, id, self.next_direction);
                    self.root = Some(new_root);
                    self.next_direction = self.next_direction.toggle();
                }
            }
        }
    }

    /// Remove a window from the layout
    pub fn remove_window(&mut self, id: WindowId) {
        if let Some(root) = &self.root {
            self.root = root.remove(id);
        }
    }

    /// Calculate rectangles for all windows
    pub fn calculate_rects(&self) -> HashMap<WindowId, Rect> {
        match &self.root {
            None => HashMap::new(),
            Some(root) => {
                // Apply margin to bounds
                let inner_bounds = self.bounds.inset(self.margin);
                let mut rects = root.calculate_rects(inner_bounds);

                // Apply gap to each rect
                if self.gap > 0.0 {
                    let half_gap = self.gap / 2.0;
                    for rect in rects.values_mut() {
                        *rect = rect.inset(half_gap);
                    }
                }

                rects
            }
        }
    }

    /// Get the number of windows
    pub fn window_count(&self) -> usize {
        self.root.as_ref().map(|r| r.window_count()).unwrap_or(0)
    }

    /// Check if a window is in the layout
    pub fn contains(&self, id: WindowId) -> bool {
        self.root.as_ref().map(|r| r.contains(id)).unwrap_or(false)
    }

    /// Get all window IDs
    pub fn window_ids(&self) -> Vec<WindowId> {
        self.root
            .as_ref()
            .map(|r| r.window_ids())
            .unwrap_or_default()
    }

    /// Get the root node
    pub fn root(&self) -> Option<&LayoutNode> {
        self.root.as_ref()
    }

    /// Adjust the split ratio at the root
    pub fn adjust_root_ratio(&mut self, delta: f32) {
        if let Some(LayoutNode::Split { ratio, .. }) = &mut self.root {
            *ratio = (*ratio + delta).clamp(0.1, 0.9);
        }
    }

    /// Swap the positions of two windows
    pub fn swap_windows(&mut self, id1: WindowId, id2: WindowId) {
        if let Some(root) = &mut self.root {
            Self::swap_in_node(root, id1, id2);
        }
    }

    fn swap_in_node(node: &mut LayoutNode, id1: WindowId, id2: WindowId) {
        match node {
            LayoutNode::Window(id) => {
                if *id == id1 {
                    *id = id2;
                } else if *id == id2 {
                    *id = id1;
                }
            }
            LayoutNode::Split { first, second, .. } => {
                Self::swap_in_node(first, id1, id2);
                Self::swap_in_node(second, id1, id2);
            }
        }
    }

    /// Rotate the layout (swap first and second in splits)
    pub fn rotate(&mut self) {
        if let Some(root) = &mut self.root {
            Self::rotate_node(root);
        }
    }

    fn rotate_node(node: &mut LayoutNode) {
        if let LayoutNode::Split { first, second, .. } = node {
            std::mem::swap(first, second);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_window() {
        let mut layout = TilingLayout::new(Rect::new(0.0, 0.0, 800.0, 600.0));
        layout.set_gap(0.0);
        layout.set_margin(0.0);

        let id = WindowId(1);
        layout.add_window(id);

        let rects = layout.calculate_rects();
        assert_eq!(rects.len(), 1);

        let rect = rects.get(&id).unwrap();
        assert_eq!(rect.width, 800.0);
        assert_eq!(rect.height, 600.0);
    }

    #[test]
    fn test_two_windows_horizontal() {
        let mut layout = TilingLayout::new(Rect::new(0.0, 0.0, 800.0, 600.0));
        layout.set_gap(0.0);
        layout.set_margin(0.0);

        let id1 = WindowId(1);
        let id2 = WindowId(2);
        layout.add_window(id1);
        layout.add_window(id2);

        let rects = layout.calculate_rects();
        assert_eq!(rects.len(), 2);

        let rect1 = rects.get(&id1).unwrap();
        let rect2 = rects.get(&id2).unwrap();

        // Horizontal split: both have full height, half width
        assert_eq!(rect1.width, 400.0);
        assert_eq!(rect2.width, 400.0);
        assert_eq!(rect1.height, 600.0);
        assert_eq!(rect2.height, 600.0);
    }

    #[test]
    fn test_three_windows() {
        let mut layout = TilingLayout::new(Rect::new(0.0, 0.0, 800.0, 600.0));
        layout.set_gap(0.0);
        layout.set_margin(0.0);

        let id1 = WindowId(1);
        let id2 = WindowId(2);
        let id3 = WindowId(3);
        layout.add_window(id1);
        layout.add_window(id2);
        layout.add_window(id3);

        let rects = layout.calculate_rects();
        assert_eq!(rects.len(), 3);

        // First window: left half
        let rect1 = rects.get(&id1).unwrap();
        assert_eq!(rect1.width, 400.0);
        assert_eq!(rect1.height, 600.0);

        // Second and third: right half, split vertically
        let rect2 = rects.get(&id2).unwrap();
        let rect3 = rects.get(&id3).unwrap();
        assert_eq!(rect2.width, 400.0);
        assert_eq!(rect3.width, 400.0);
        assert_eq!(rect2.height, 300.0);
        assert_eq!(rect3.height, 300.0);
    }

    #[test]
    fn test_remove_window() {
        let mut layout = TilingLayout::new(Rect::new(0.0, 0.0, 800.0, 600.0));
        layout.set_gap(0.0);
        layout.set_margin(0.0);

        let id1 = WindowId(1);
        let id2 = WindowId(2);
        layout.add_window(id1);
        layout.add_window(id2);

        layout.remove_window(id1);

        let rects = layout.calculate_rects();
        assert_eq!(rects.len(), 1);

        // Remaining window fills the space
        let rect2 = rects.get(&id2).unwrap();
        assert_eq!(rect2.width, 800.0);
        assert_eq!(rect2.height, 600.0);
    }

    #[test]
    fn test_gap_and_margin() {
        let mut layout = TilingLayout::new(Rect::new(0.0, 0.0, 800.0, 600.0));
        layout.set_gap(10.0);
        layout.set_margin(20.0);

        let id = WindowId(1);
        layout.add_window(id);

        let rects = layout.calculate_rects();
        let rect = rects.get(&id).unwrap();

        // Margin of 20 on each side, plus half gap (5) = 25
        assert_eq!(rect.x, 25.0);
        assert_eq!(rect.y, 25.0);
        assert_eq!(rect.width, 800.0 - 50.0);
        assert_eq!(rect.height, 600.0 - 50.0);
    }

    #[test]
    fn test_contains() {
        let mut layout = TilingLayout::new(Rect::new(0.0, 0.0, 800.0, 600.0));

        let id1 = WindowId(1);
        let id2 = WindowId(2);
        let id3 = WindowId(3);

        layout.add_window(id1);
        layout.add_window(id2);

        assert!(layout.contains(id1));
        assert!(layout.contains(id2));
        assert!(!layout.contains(id3));
    }

    #[test]
    fn test_swap_windows() {
        let mut layout = TilingLayout::new(Rect::new(0.0, 0.0, 800.0, 600.0));
        layout.set_gap(0.0);
        layout.set_margin(0.0);

        let id1 = WindowId(1);
        let id2 = WindowId(2);
        layout.add_window(id1);
        layout.add_window(id2);

        // Get original positions
        let rects_before = layout.calculate_rects();
        let rect1_before = *rects_before.get(&id1).unwrap();
        let rect2_before = *rects_before.get(&id2).unwrap();

        // Swap
        layout.swap_windows(id1, id2);

        // Check positions swapped
        let rects_after = layout.calculate_rects();
        let rect1_after = *rects_after.get(&id1).unwrap();
        let rect2_after = *rects_after.get(&id2).unwrap();

        assert_eq!(rect1_after.x, rect2_before.x);
        assert_eq!(rect2_after.x, rect1_before.x);
    }
}
