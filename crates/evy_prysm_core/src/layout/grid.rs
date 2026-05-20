//! Grid container layout: organelles in rows and columns.
//!
//! Per prysm/layout.md §6.2. Children fill cells in row-major order
//! (left to right, top to bottom). Each child occupies one cell; cells
//! that have no child render empty space.
//!
//! Track resolution (per dimension): fix → scale → fill, same as
//! container sizing.
//!
//! Session 7.2 scope:
//! - Row-major child placement (no `areas` declarations yet)
//! - Track resolution with fix/scale/fill
//! - Independent col_gap and row_gap

use super::{layout_recursive, LayoutResult};
use crate::constraint::{Constraint, OccupiedSize, Position};
use crate::element::Element;
use crate::sizing::SizeType;

/// Resolve one axis of tracks (columns or rows) into concrete sizes in quanta.
///
/// Two-pass: fix + scale consume; fill divides the remainder by weight.
fn resolve_tracks(tracks: &[SizeType], avail: u32, gap: u32) -> Vec<u32> {
    let n = tracks.len();
    if n == 0 {
        return Vec::new();
    }
    let total_gap = gap.saturating_mul((n as u32).saturating_sub(1));
    let usable = avail.saturating_sub(total_gap);

    let mut sizes: Vec<u32> = vec![0; n];
    let mut fill_indices: Vec<usize> = Vec::new();
    let mut fill_weight_total: f32 = 0.0;
    let mut fixed_consumed: u32 = 0;

    for (i, t) in tracks.iter().enumerate() {
        match t {
            SizeType::Fix(k) => {
                let s = (*k).min(usable.saturating_sub(fixed_consumed));
                sizes[i] = s;
                fixed_consumed = fixed_consumed.saturating_add(s);
            }
            SizeType::Scale { ratio, min } => {
                let scaled = (usable as f32 * ratio).round() as u32;
                let s = scaled.max(*min).min(usable.saturating_sub(fixed_consumed));
                sizes[i] = s;
                fixed_consumed = fixed_consumed.saturating_add(s);
            }
            SizeType::Fill { weight } => {
                fill_indices.push(i);
                fill_weight_total += *weight;
            }
        }
    }

    let remainder = usable.saturating_sub(fixed_consumed);
    if !fill_indices.is_empty() && fill_weight_total > 0.0 {
        let mut distributed: u32 = 0;
        for (k, &i) in fill_indices.iter().enumerate() {
            let weight = match &tracks[i] {
                SizeType::Fill { weight } => *weight,
                _ => 0.0,
            };
            let share = if k + 1 == fill_indices.len() {
                remainder.saturating_sub(distributed)
            } else {
                ((remainder as f32) * weight / fill_weight_total).round() as u32
            };
            sizes[i] = share;
            distributed = distributed.saturating_add(share);
        }
    }

    sizes
}

/// Place children in a grid by row-major order. Each child gets a
/// constraint equal to its cell size; the child's own sizing decides
/// how much of the cell it actually occupies.
#[allow(clippy::too_many_arguments)]
pub(super) fn layout(
    element: &Element,
    parent_size: OccupiedSize,
    parent_pos: Position,
    columns: &[SizeType],
    rows: &[SizeType],
    col_gap: u32,
    row_gap: u32,
    result: &mut LayoutResult,
    next_id: &mut usize,
) {
    if element.children.is_empty() {
        return;
    }
    if columns.is_empty() || rows.is_empty() {
        // Degenerate grid: process children as zero-sized placements.
        for child in &element.children {
            layout_recursive(child, Constraint::new(0, 0), parent_pos, result, next_id);
        }
        return;
    }

    let col_sizes = resolve_tracks(columns, parent_size.w, col_gap);
    let row_sizes = resolve_tracks(rows, parent_size.h, row_gap);

    let n_cols = col_sizes.len();
    let n_rows = row_sizes.len();
    let total_cells = n_cols * n_rows;

    // Pre-compute cumulative cell origins (x, y) per (col, row).
    let mut col_x: Vec<u32> = Vec::with_capacity(n_cols);
    let mut acc = 0u32;
    for (i, &w) in col_sizes.iter().enumerate() {
        col_x.push(acc);
        acc = acc.saturating_add(w);
        if i + 1 < n_cols {
            acc = acc.saturating_add(col_gap);
        }
    }
    let mut row_y: Vec<u32> = Vec::with_capacity(n_rows);
    let mut acc = 0u32;
    for (i, &h) in row_sizes.iter().enumerate() {
        row_y.push(acc);
        acc = acc.saturating_add(h);
        if i + 1 < n_rows {
            acc = acc.saturating_add(row_gap);
        }
    }

    // Place children row-major. Children beyond `total_cells` are
    // ignored at the placement step but still consume an ID slot so
    // the LayoutResult indices stay aligned with the tree.
    for (idx, child) in element.children.iter().enumerate() {
        if idx < total_cells {
            let col = idx % n_cols;
            let row = idx / n_cols;
            let cell_w = col_sizes[col];
            let cell_h = row_sizes[row];
            let pos = Position::new(parent_pos.x + col_x[col], parent_pos.y + row_y[row]);
            let constraint = Constraint::new(cell_w, cell_h);
            layout_recursive(child, constraint, pos, result, next_id);
        } else {
            layout_recursive(
                child,
                Constraint::new(0, 0),
                parent_pos,
                result,
                next_id,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::container::Container;
    use crate::element::Element;
    use crate::layout::layout;
    use crate::sizing::{Size, SizeType};
    use crate::{Constraint, OccupiedSize, Position};

    fn leaf(w: u32, h: u32) -> Element {
        Element::leaf(Size::new(SizeType::Fix(w), SizeType::Fix(h)))
    }

    fn grid_2x2(children: Vec<Element>) -> Element {
        Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::Grid {
                columns: vec![SizeType::fill(), SizeType::fill()],
                rows: vec![SizeType::fill(), SizeType::fill()],
                col_gap: 1,
                row_gap: 1,
            },
            children,
        )
    }

    #[test]
    fn grid_places_children_row_major() {
        let g = grid_2x2(vec![leaf(5, 5), leaf(5, 5), leaf(5, 5), leaf(5, 5)]);
        // 100x100, two columns (each ~49 after 1px gap) and two rows.
        let r = layout(&g, Constraint::new(100, 100));
        assert_eq!(r.len(), 5);
        // Top-left at (0,0); top-right after first column + gap.
        assert_eq!(r.positions[1], Position::new(0, 0));
        let col1_offset = r.positions[2].x;
        assert!(col1_offset >= 49 && col1_offset <= 51, "col1 x ≈ 50");
        // Bottom row offset by row height + 1 gap.
        let row1_offset = r.positions[3].y;
        assert!(row1_offset >= 49 && row1_offset <= 51, "row1 y ≈ 50");
        // Bottom-right combines both.
        assert_eq!(r.positions[4].x, col1_offset);
        assert_eq!(r.positions[4].y, row1_offset);
    }

    #[test]
    fn grid_resolves_fix_columns() {
        let g = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::Grid {
                columns: vec![SizeType::Fix(30), SizeType::fill()],
                rows: vec![SizeType::Fix(20)],
                col_gap: 0,
                row_gap: 0,
            },
            vec![leaf(10, 10), leaf(10, 10)],
        );
        let r = layout(&g, Constraint::new(100, 100));
        // First column = 30, second = fill = 70.
        assert_eq!(r.positions[1], Position::new(0, 0));
        assert_eq!(r.positions[2].x, 30);
    }

    #[test]
    fn grid_with_more_children_than_cells_doesnt_panic() {
        let g = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::Grid {
                columns: vec![SizeType::fill()],
                rows: vec![SizeType::fill()],
                col_gap: 0,
                row_gap: 0,
            },
            vec![leaf(10, 10), leaf(10, 10), leaf(10, 10)],
        );
        let r = layout(&g, Constraint::new(100, 100));
        // 4 elements total (1 grid + 3 children). No panic.
        assert_eq!(r.len(), 4);
    }

    #[test]
    fn grid_with_no_columns_doesnt_panic() {
        let g = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::Grid {
                columns: vec![],
                rows: vec![SizeType::fill()],
                col_gap: 0,
                row_gap: 0,
            },
            vec![leaf(10, 10)],
        );
        let _ = layout(&g, Constraint::new(100, 100));
    }

    #[test]
    fn grid_empty_lays_out_only_self() {
        let g = Element::membrane(
            Size::new(SizeType::fill(), SizeType::fill()),
            Container::Grid {
                columns: vec![SizeType::fill()],
                rows: vec![SizeType::fill()],
                col_gap: 0,
                row_gap: 0,
            },
            vec![],
        );
        let r = layout(&g, Constraint::new(100, 100));
        assert_eq!(r.len(), 1);
        assert_eq!(r.sizes[0], OccupiedSize::new(100, 100));
    }
}
