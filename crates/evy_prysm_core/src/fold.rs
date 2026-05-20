//! Fold protocol — responsive conformations.
//!
//! Per prysm/layout.md §4.3, every molecule defines `F` = ordered set of
//! conformations:
//!
//! `F = {l_1, l_2, ..., l_k}` where `w_min(l_1) > w_min(l_2) > ... > w_min(l_k)`
//!
//! Selection at runtime:
//!
//! `l* = l_i  where i = min{j : w_min(l_j) ≤ c_w}`
//!
//! Theorem 6 (derived in spec §4.3a): for a stack with `m` organelles each
//! with importance `φ*_i` and width `w_i`, the optimal fold set is computed
//! in `O(m log m)` by greedily removing the least important remaining
//! organelle.

/// One layout conformation in a fold set. Includes a subset of children
/// (by their indices in the parent's `Element.children` Vec) and the
/// minimum constraint width at which this conformation fits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conformation {
    /// Indices into the parent `Element::children` Vec for the children
    /// included in this conformation. Indices preserve original order.
    pub child_indices: Vec<usize>,
    /// Minimum main-axis constraint at which this conformation fits, in quanta.
    pub w_min: u32,
}

/// Ordered fold set — conformations from widest to narrowest by `w_min`.
#[derive(Debug, Clone, Default)]
pub struct FoldSet {
    /// Conformations ordered by decreasing `w_min`. Index 0 is the widest
    /// (full set); the last index is the narrowest (smallest subset).
    pub conformations: Vec<Conformation>,
}

impl FoldSet {
    /// Select the active conformation for a constraint of width `c_w`.
    ///
    /// Returns the index of the conformation whose `w_min ≤ c_w` and is
    /// the widest among those that fit. Per spec §4.3: `i = min{j : w_min(l_j) ≤ c_w}`
    /// when conformations are ordered widest-first (decreasing w_min).
    ///
    /// If no conformation fits, returns the narrowest available
    /// (caller's choice: clip, scroll, or render best-effort).
    pub fn select(&self, c_w: u32) -> usize {
        for (i, conf) in self.conformations.iter().enumerate() {
            if conf.w_min <= c_w {
                return i;
            }
        }
        self.conformations.len().saturating_sub(1)
    }

    /// Return the active conformation by reference, if any exist.
    pub fn active(&self, c_w: u32) -> Option<&Conformation> {
        if self.conformations.is_empty() {
            None
        } else {
            Some(&self.conformations[self.select(c_w)])
        }
    }

    /// Derive an optimal fold set per spec Theorem 6.
    ///
    /// `widths[i]` is the main-axis width of organelle i in quanta.
    /// `importances[i]` is the scalar importance `φ*_i` (e.g. cyberank,
    /// focus, or manual priority). `gap` is the inter-organelle gap.
    ///
    /// Returns m conformations: l_1 includes all; l_{j+1} = l_j minus the
    /// least important remaining organelle; l_m has just the single most
    /// important organelle.
    ///
    /// Panics if `widths.len() != importances.len()`.
    pub fn derive_optimal(widths: &[u32], importances: &[f32], gap: u32) -> Self {
        assert_eq!(
            widths.len(),
            importances.len(),
            "widths and importances must have the same length"
        );
        let m = widths.len();
        if m == 0 {
            return Self::default();
        }

        // Sort original indices by importance ascending (least important first).
        let mut indices_by_imp: Vec<usize> = (0..m).collect();
        indices_by_imp.sort_by(|&a, &b| {
            importances[a]
                .partial_cmp(&importances[b])
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // l_1 = full set, w_min = Σ widths + (m-1)*gap.
        let mut current: Vec<usize> = (0..m).collect();
        let mut current_w_min: u32 = widths
            .iter()
            .sum::<u32>()
            .saturating_add(gap.saturating_mul((m as u32).saturating_sub(1)));

        let mut conformations: Vec<Conformation> = Vec::with_capacity(m);
        conformations.push(Conformation {
            child_indices: current.clone(),
            w_min: current_w_min,
        });

        // Greedy: remove least important remaining organelle.
        for j in 0..m.saturating_sub(1) {
            let to_remove = indices_by_imp[j];
            current.retain(|&i| i != to_remove);
            current_w_min = current_w_min
                .saturating_sub(widths[to_remove])
                .saturating_sub(gap);
            conformations.push(Conformation {
                child_indices: current.clone(),
                w_min: current_w_min,
            });
        }

        Self { conformations }
    }

    /// Build a fold set manually from explicit conformations. The caller
    /// must ensure they are ordered by decreasing `w_min`.
    pub fn from_conformations(conformations: Vec<Conformation>) -> Self {
        Self { conformations }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_optimal_with_three_organelles() {
        // 3 organelles with widths [10, 20, 30] and importances [1, 2, 3].
        // Sorted by importance ascending: indices [0, 1, 2].
        // Conformations:
        //   l_1: {0,1,2}, w_min = 10+20+30 + 2*gap
        //   l_2: {1,2},   w_min = 20+30 + 1*gap (removed 0)
        //   l_3: {2},     w_min = 30 + 0*gap (removed 1)
        let fs = FoldSet::derive_optimal(&[10, 20, 30], &[1.0, 2.0, 3.0], 2);
        assert_eq!(fs.conformations.len(), 3);

        assert_eq!(fs.conformations[0].child_indices, vec![0, 1, 2]);
        assert_eq!(fs.conformations[0].w_min, 10 + 20 + 30 + 2 * 2);

        assert_eq!(fs.conformations[1].child_indices, vec![1, 2]);
        assert_eq!(fs.conformations[1].w_min, 20 + 30 + 2);

        assert_eq!(fs.conformations[2].child_indices, vec![2]);
        assert_eq!(fs.conformations[2].w_min, 30);
    }

    #[test]
    fn select_picks_widest_that_fits() {
        let fs = FoldSet::derive_optimal(&[10, 20, 30], &[1.0, 2.0, 3.0], 2);
        // c_w = 100: l_1 (w_min=64) fits.
        assert_eq!(fs.select(100), 0);
        // c_w = 50: l_2 (w_min=52) does not fit, l_3 (w_min=30) fits.
        assert_eq!(fs.select(50), 2);
        // c_w = 52: l_2 just fits.
        assert_eq!(fs.select(52), 1);
        // c_w = 30: only l_3 fits.
        assert_eq!(fs.select(30), 2);
        // c_w = 10: nothing fits; returns narrowest.
        assert_eq!(fs.select(10), 2);
    }

    #[test]
    fn derive_optimal_handles_equal_importance() {
        // When importances tie, sort is stable in std (for the tie-break we
        // rely on stable sort of (importance, index) — but partial_cmp may
        // not be strictly stable). Just verify the algorithm doesn't crash
        // and produces a valid fold set.
        let fs = FoldSet::derive_optimal(&[10, 10, 10], &[1.0, 1.0, 1.0], 1);
        assert_eq!(fs.conformations.len(), 3);
        assert_eq!(fs.conformations[0].child_indices.len(), 3);
        assert_eq!(fs.conformations[2].child_indices.len(), 1);
    }

    #[test]
    fn empty_input_yields_empty_fold_set() {
        let fs = FoldSet::derive_optimal(&[], &[], 0);
        assert!(fs.conformations.is_empty());
        assert!(fs.active(100).is_none());
    }

    #[test]
    fn single_organelle_yields_one_conformation() {
        let fs = FoldSet::derive_optimal(&[10], &[1.0], 0);
        assert_eq!(fs.conformations.len(), 1);
        assert_eq!(fs.conformations[0].child_indices, vec![0]);
        assert_eq!(fs.conformations[0].w_min, 10);
    }

    #[test]
    fn theorem6_invariant_widths_decrease() {
        // Spec invariant: conformations ordered by decreasing w_min.
        let fs = FoldSet::derive_optimal(&[5, 10, 15, 20], &[4.0, 3.0, 2.0, 1.0], 1);
        for i in 1..fs.conformations.len() {
            assert!(
                fs.conformations[i - 1].w_min > fs.conformations[i].w_min,
                "conformations must have strictly decreasing w_min"
            );
        }
    }
}
