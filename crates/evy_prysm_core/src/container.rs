//! Container axiom K: stack, grid, layer.
//!
//! See prysm/layout.md §6.
//!
//! Session 1 implements `Stack`. `Grid` and `Layer` are defined here so
//! downstream code can declare them, but their layout passes are stubbed
//! (panic on use) until session 2.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Align {
    Start,
    Center,
    End,
    Stretch,
}

/// Container topology. One of three spatial relations between an
/// element's organelles.
#[derive(Debug, Clone, PartialEq)]
pub enum Container {
    /// Organelles along a single axis. Most common.
    Stack {
        direction: Direction,
        /// Gap between adjacent organelles, in quanta.
        gap: u32,
        /// Cross-axis alignment.
        align: Align,
    },
    /// Organelles in rows and columns. Session 2.
    Grid {
        /// Track sizes for each column. Empty = single column.
        columns: Vec<crate::SizeType>,
        /// Track sizes for each row.
        rows: Vec<crate::SizeType>,
        /// Gaps between tracks.
        col_gap: u32,
        row_gap: u32,
    },
    /// Organelles share the same region, ordered by depth. Session 2.
    Layer,
}

impl Container {
    /// Convenience constructor for a vertical stack with default gap and align.
    pub fn vertical() -> Self {
        Self::Stack {
            direction: Direction::Vertical,
            gap: 1,
            align: Align::Start,
        }
    }

    /// Convenience constructor for a horizontal stack with default gap and align.
    pub fn horizontal() -> Self {
        Self::Stack {
            direction: Direction::Horizontal,
            gap: 1,
            align: Align::Start,
        }
    }
}
