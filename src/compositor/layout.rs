//! Tiling layout algorithm
//!
//! Binary space partitioning for window layout.
//! Windows never overlap - the screen is recursively split.

use super::{Rect, WindowId};
use std::collections::HashMap;

/// Split direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Split {
    Horizontal, // Left | Right
    Vertical,   // Top / Bottom
}

/// A node in the layout tree
#[derive(Debug, Clone)]
pub enum LayoutNode {
    /// A leaf node containing a window
    Window(WindowId),

    /// A split containing two children
    Split {
        direction: Split,
        ratio: f32, // 0.0 - 1.0, where first child gets `ratio` of space
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },

    /// Empty slot (can be filled)
    Empty,
}

impl LayoutNode {
    /// Calculate rectangles for all windows in this subtree
    pub fn calculate(&self, bounds: Rect, out: &mut HashMap<WindowId, Rect>) {
        match self {
            LayoutNode::Window(id) => {
                out.insert(*id, bounds);
            }
            LayoutNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let (first_bounds, second_bounds) = match direction {
                    Split::Horizontal => bounds.split_horizontal(*ratio),
                    Split::Vertical => bounds.split_vertical(*ratio),
                };
                first.calculate(first_bounds, out);
                second.calculate(second_bounds, out);
            }
            LayoutNode::Empty => {}
        }
    }

    /// Count windows in this subtree
    pub fn window_count(&self) -> usize {
        match self {
            LayoutNode::Window(_) => 1,
            LayoutNode::Split { first, second, .. } => {
                first.window_count() + second.window_count()
            }
            LayoutNode::Empty => 0,
        }
    }

    /// Find and remove a window, returning true if found
    pub fn remove_window(&mut self, id: WindowId) -> bool {
        match self {
            LayoutNode::Window(wid) if *wid == id => {
                *self = LayoutNode::Empty;
                true
            }
            LayoutNode::Split { first, second, .. } => {
                if first.remove_window(id) {
                    // Promote second child if first is now empty
                    if matches!(**first, LayoutNode::Empty) {
                        *self = (**second).clone();
                    }
                    true
                } else if second.remove_window(id) {
                    // Promote first child if second is now empty
                    if matches!(**second, LayoutNode::Empty) {
                        *self = (**first).clone();
                    }
                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    /// Find first empty slot and fill it
    pub fn fill_empty(&mut self, id: WindowId) -> bool {
        match self {
            LayoutNode::Empty => {
                *self = LayoutNode::Window(id);
                true
            }
            LayoutNode::Split { first, second, .. } => {
                first.fill_empty(id) || second.fill_empty(id)
            }
            LayoutNode::Window(_) => false,
        }
    }

    /// Add a window by splitting the last window
    pub fn add_window(&mut self, id: WindowId, prefer_direction: Split) -> bool {
        match self {
            LayoutNode::Empty => {
                *self = LayoutNode::Window(id);
                true
            }
            LayoutNode::Window(existing) => {
                let existing_id = *existing;
                *self = LayoutNode::Split {
                    direction: prefer_direction,
                    ratio: 0.5,
                    first: Box::new(LayoutNode::Window(existing_id)),
                    second: Box::new(LayoutNode::Window(id)),
                };
                true
            }
            LayoutNode::Split { first, second, direction, .. } => {
                // Alternate split direction for balanced layout
                let next_direction = match direction {
                    Split::Horizontal => Split::Vertical,
                    Split::Vertical => Split::Horizontal,
                };

                // Add to the smaller subtree
                if first.window_count() <= second.window_count() {
                    first.add_window(id, next_direction)
                } else {
                    second.add_window(id, next_direction)
                }
            }
        }
    }
}

/// The layout manager
pub struct Layout {
    root: LayoutNode,
    /// Preferred initial split direction
    prefer_direction: Split,
}

impl Layout {
    pub fn new() -> Self {
        Self {
            root: LayoutNode::Empty,
            prefer_direction: Split::Horizontal,
        }
    }

    /// Add a window to the layout
    pub fn add_window(&mut self, id: WindowId) {
        self.root.add_window(id, self.prefer_direction);
    }

    /// Remove a window from the layout
    pub fn remove_window(&mut self, id: WindowId) {
        self.root.remove_window(id);
    }

    /// Calculate window rectangles for a given screen size
    pub fn calculate(&self, screen: Rect) -> HashMap<WindowId, Rect> {
        let mut result = HashMap::new();
        self.root.calculate(screen, &mut result);
        result
    }

    /// Number of windows
    pub fn window_count(&self) -> usize {
        self.root.window_count()
    }

    /// Is layout empty?
    pub fn is_empty(&self) -> bool {
        matches!(self.root, LayoutNode::Empty)
    }
}

impl Default for Layout {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_layout() {
        let layout = Layout::new();
        assert!(layout.is_empty());
        assert_eq!(layout.window_count(), 0);

        let rects = layout.calculate(Rect::new(0.0, 0.0, 800.0, 600.0));
        assert!(rects.is_empty());
    }

    #[test]
    fn test_single_window() {
        let mut layout = Layout::new();
        let id = WindowId(0);

        layout.add_window(id);

        assert_eq!(layout.window_count(), 1);

        let rects = layout.calculate(Rect::new(0.0, 0.0, 800.0, 600.0));
        assert_eq!(rects.len(), 1);

        let rect = rects.get(&id).unwrap();
        assert_eq!(rect.x, 0.0);
        assert_eq!(rect.y, 0.0);
        assert_eq!(rect.width, 800.0);
        assert_eq!(rect.height, 600.0);
    }

    #[test]
    fn test_two_windows_split() {
        let mut layout = Layout::new();
        let id1 = WindowId(0);
        let id2 = WindowId(1);

        layout.add_window(id1);
        layout.add_window(id2);

        assert_eq!(layout.window_count(), 2);

        let rects = layout.calculate(Rect::new(0.0, 0.0, 800.0, 600.0));
        assert_eq!(rects.len(), 2);

        let rect1 = rects.get(&id1).unwrap();
        let rect2 = rects.get(&id2).unwrap();

        // Should be split horizontally
        assert_eq!(rect1.width, 400.0);
        assert_eq!(rect2.width, 400.0);
        assert_eq!(rect1.height, 600.0);
        assert_eq!(rect2.height, 600.0);
    }

    #[test]
    fn test_remove_window() {
        let mut layout = Layout::new();
        let id1 = WindowId(0);
        let id2 = WindowId(1);

        layout.add_window(id1);
        layout.add_window(id2);
        layout.remove_window(id1);

        assert_eq!(layout.window_count(), 1);

        let rects = layout.calculate(Rect::new(0.0, 0.0, 800.0, 600.0));
        assert_eq!(rects.len(), 1);
        assert!(rects.contains_key(&id2));
    }

    #[test]
    fn test_four_windows() {
        let mut layout = Layout::new();

        for i in 0..4 {
            layout.add_window(WindowId(i));
        }

        assert_eq!(layout.window_count(), 4);

        let rects = layout.calculate(Rect::new(0.0, 0.0, 800.0, 600.0));
        assert_eq!(rects.len(), 4);

        // Each window should have roughly equal area
        for (_, rect) in &rects {
            let area = rect.width * rect.height;
            assert!(area > 100000.0); // Should be reasonably sized
        }
    }

    #[test]
    fn test_rect_contains() {
        let rect = Rect::new(10.0, 20.0, 100.0, 50.0);

        assert!(rect.contains(10.0, 20.0));
        assert!(rect.contains(50.0, 40.0));
        assert!(rect.contains(109.0, 69.0));

        assert!(!rect.contains(9.0, 20.0));
        assert!(!rect.contains(110.0, 20.0));
        assert!(!rect.contains(50.0, 70.0));
    }

    #[test]
    fn test_rect_split_horizontal() {
        let rect = Rect::new(0.0, 0.0, 100.0, 50.0);
        let (left, right) = rect.split_horizontal(0.3);

        assert_eq!(left.x, 0.0);
        // Use approximate comparison for floating point
        assert!((left.width - 30.0).abs() < 0.001);
        assert!((right.x - 30.0).abs() < 0.001);
        assert!((right.width - 70.0).abs() < 0.001);
    }

    #[test]
    fn test_rect_split_vertical() {
        let rect = Rect::new(0.0, 0.0, 100.0, 50.0);
        let (top, bottom) = rect.split_vertical(0.5);

        assert_eq!(top.y, 0.0);
        assert_eq!(top.height, 25.0);
        assert_eq!(bottom.y, 25.0);
        assert_eq!(bottom.height, 25.0);
    }
}
