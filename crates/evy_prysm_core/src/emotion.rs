//! Emotion — computed color signal layer.
//!
//! Per prysm/specs/emotion.md: 9 palette values (7 spectrum emotions +
//! neutral + inactive) computed via four evaluation rules:
//!
//! - polarity: sign(v) → red / white / green
//! - threshold: v vs cutoffs → green / yellow / red
//! - semantic: action category → categorical lookup
//! - continuous: v ∈ [min, max] → spectrum position
//!
//! Emotion is never assigned manually — always derived from data. The
//! rule: emotion must never lie. Stale data fades, missing data shows
//! neutral, never false signal.

/// 9 palette values: 7 color-emotion spectrum + neutral + inactive.
/// Hex codes per prysm/specs/emotion.md §"the palette".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmotionColor {
    /// Anger — red #ff0000. Danger, critical failure, overload.
    Anger,
    /// Disgust — orange #ff5b00. Contamination, invalid data, rejection.
    Disgust,
    /// Surprise — yellow #fcf000. Attention, sudden change, caution.
    Surprise,
    /// Joy — green #00fe00. Confidence, success, growth, life.
    Joy,
    /// Interest — blue #00acff. Exploration, focus, discovery.
    Interest,
    /// Sadness — indigo #304ffe. Withdrawal, loss, inactivity.
    Sadness,
    /// Fear — violet #d500f9. Unknown threat, radiation, boundary.
    Fear,
    /// Neutral — white #ffffff. No signal, idle, default.
    Neutral,
    /// Inactive — gray #4b4b4d. Disabled, off, muted.
    Inactive,
}

impl EmotionColor {
    /// Return the 24-bit RGB hex code per the palette.
    pub const fn hex(self) -> u32 {
        match self {
            Self::Anger => 0xff0000,
            Self::Disgust => 0xff5b00,
            Self::Surprise => 0xfcf000,
            Self::Joy => 0x00fe00,
            Self::Interest => 0x00acff,
            Self::Sadness => 0x304ffe,
            Self::Fear => 0xd500f9,
            Self::Neutral => 0xffffff,
            Self::Inactive => 0x4b4b4d,
        }
    }

    /// Return (r, g, b) bytes in 0..=255.
    pub const fn rgb(self) -> (u8, u8, u8) {
        let h = self.hex();
        (((h >> 16) & 0xff) as u8, ((h >> 8) & 0xff) as u8, (h & 0xff) as u8)
    }
}

/// Polarity rule: sign of `v` → green / white / red.
/// Used by counters with delta values, balance changes, etc.
pub fn polarity(v: f64) -> EmotionColor {
    if v > 0.0 {
        EmotionColor::Joy
    } else if v < 0.0 {
        EmotionColor::Anger
    } else {
        EmotionColor::Neutral
    }
}

/// Three-zone threshold rule.
/// `v >= high` → green; `low <= v < high` → yellow; `v < low` → red.
pub fn threshold(v: f64, low: f64, high: f64) -> EmotionColor {
    if v >= high {
        EmotionColor::Joy
    } else if v >= low {
        EmotionColor::Surprise
    } else {
        EmotionColor::Anger
    }
}

/// N-zone threshold: ordered cutoffs paired with their result colors.
///
/// Returns `colors[i]` where `i = max { j : v >= cutoffs[j] }`. If `v`
/// is below all cutoffs, returns `colors[0]`.
///
/// `cutoffs` and `colors` must have the same length and `cutoffs` must
/// be sorted ascending. Empty `cutoffs` returns `Neutral`.
pub fn threshold_multi(v: f64, cutoffs: &[f64], colors: &[EmotionColor]) -> EmotionColor {
    debug_assert_eq!(cutoffs.len(), colors.len(), "cutoffs and colors must align");
    if cutoffs.is_empty() {
        return EmotionColor::Neutral;
    }
    let mut idx = 0usize;
    for (j, &c) in cutoffs.iter().enumerate() {
        if v >= c {
            idx = j;
        } else {
            break;
        }
    }
    colors[idx]
}

/// Categorical action types for the semantic rule. Per prysm/specs/emotion.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticAction {
    /// Navigate — open, browse, expand. White.
    Navigate,
    /// Confirm — sign, stake, claim, delegate. Green.
    Confirm,
    /// Danger — delete key, burn tokens. Red.
    Danger,
    /// Caution — large transfer, irreversible. Yellow.
    Caution,
    /// Explore — search, discover, ask. Blue.
    Explore,
}

/// Semantic rule: action category → emotion.
pub fn semantic(action: SemanticAction) -> EmotionColor {
    match action {
        SemanticAction::Navigate => EmotionColor::Neutral,
        SemanticAction::Confirm => EmotionColor::Joy,
        SemanticAction::Danger => EmotionColor::Anger,
        SemanticAction::Caution => EmotionColor::Surprise,
        SemanticAction::Explore => EmotionColor::Interest,
    }
}

/// Continuous rule: map `v ∈ [min, max]` to spectrum position. `0` →
/// red, `0.5` → green, `1.0` → violet. Interpolation between adjacent
/// spectrum colors is conceptual; this function snaps to the closest
/// palette color per the seven-color spectrum.
pub fn continuous(v: f64, min: f64, max: f64) -> EmotionColor {
    let range = max - min;
    if range <= 0.0 {
        return EmotionColor::Neutral;
    }
    let t = ((v - min) / range).clamp(0.0, 1.0);
    // 7 spectrum colors mapped to [0,1] in ROYGBIV order.
    let palette = [
        EmotionColor::Anger,    // 0/6
        EmotionColor::Disgust,  // 1/6
        EmotionColor::Surprise, // 2/6
        EmotionColor::Joy,      // 3/6 ← center
        EmotionColor::Interest, // 4/6
        EmotionColor::Sadness,  // 5/6
        EmotionColor::Fear,     // 6/6
    ];
    let idx = (t * 6.0).round() as usize;
    palette[idx.min(6)]
}

/// Freshness of a data source backing an emotion computation. Used by
/// the renderer to honor the "emotion must never lie" rule: stale data
/// fades; missing/error returns Neutral.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    Fresh,
    /// Stale by more than the stale threshold (default 60s). Render at
    /// 50% opacity to indicate aging.
    Stale,
    /// Source unreachable or error during compute. Replace with Neutral.
    Unavailable,
}

/// Apply freshness gating to a computed emotion. Returns the emotion to
/// display (and a hint to the renderer: opacity multiplier, where
/// applicable).
pub fn apply_freshness(emotion: EmotionColor, freshness: Freshness) -> (EmotionColor, f32) {
    match freshness {
        Freshness::Fresh => (emotion, 1.0),
        Freshness::Stale => (emotion, 0.5),
        Freshness::Unavailable => (EmotionColor::Neutral, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polarity_three_branches() {
        assert_eq!(polarity(1.0), EmotionColor::Joy);
        assert_eq!(polarity(0.0), EmotionColor::Neutral);
        assert_eq!(polarity(-1.0), EmotionColor::Anger);
    }

    #[test]
    fn threshold_three_zones() {
        // v vs (low=0.2, high=0.5)
        assert_eq!(threshold(0.8, 0.2, 0.5), EmotionColor::Joy);
        assert_eq!(threshold(0.3, 0.2, 0.5), EmotionColor::Surprise);
        assert_eq!(threshold(0.1, 0.2, 0.5), EmotionColor::Anger);
        // Boundary: v exactly at high.
        assert_eq!(threshold(0.5, 0.2, 0.5), EmotionColor::Joy);
        // Boundary: v exactly at low.
        assert_eq!(threshold(0.2, 0.2, 0.5), EmotionColor::Surprise);
    }

    #[test]
    fn threshold_multi_picks_highest_satisfied() {
        let cutoffs = [0.0, 10.0, 20.0, 30.0];
        let colors = [
            EmotionColor::Anger,
            EmotionColor::Disgust,
            EmotionColor::Surprise,
            EmotionColor::Joy,
        ];
        assert_eq!(threshold_multi(35.0, &cutoffs, &colors), EmotionColor::Joy);
        assert_eq!(threshold_multi(25.0, &cutoffs, &colors), EmotionColor::Surprise);
        assert_eq!(threshold_multi(15.0, &cutoffs, &colors), EmotionColor::Disgust);
        assert_eq!(threshold_multi(5.0, &cutoffs, &colors), EmotionColor::Anger);
        // Below first cutoff → first color (degenerate).
        assert_eq!(threshold_multi(-5.0, &cutoffs, &colors), EmotionColor::Anger);
    }

    #[test]
    fn semantic_returns_category_color() {
        assert_eq!(semantic(SemanticAction::Confirm), EmotionColor::Joy);
        assert_eq!(semantic(SemanticAction::Danger), EmotionColor::Anger);
        assert_eq!(semantic(SemanticAction::Navigate), EmotionColor::Neutral);
    }

    #[test]
    fn continuous_endpoints() {
        assert_eq!(continuous(0.0, 0.0, 1.0), EmotionColor::Anger);
        assert_eq!(continuous(1.0, 0.0, 1.0), EmotionColor::Fear);
        // Center → joy.
        assert_eq!(continuous(0.5, 0.0, 1.0), EmotionColor::Joy);
    }

    #[test]
    fn continuous_degenerate_range_yields_neutral() {
        assert_eq!(continuous(5.0, 10.0, 10.0), EmotionColor::Neutral);
    }

    #[test]
    fn freshness_fresh_passes_through() {
        let (e, op) = apply_freshness(EmotionColor::Joy, Freshness::Fresh);
        assert_eq!(e, EmotionColor::Joy);
        assert_eq!(op, 1.0);
    }

    #[test]
    fn freshness_stale_dims_opacity() {
        let (e, op) = apply_freshness(EmotionColor::Joy, Freshness::Stale);
        assert_eq!(e, EmotionColor::Joy);
        assert_eq!(op, 0.5);
    }

    #[test]
    fn freshness_unavailable_replaces_with_neutral() {
        let (e, _) = apply_freshness(EmotionColor::Joy, Freshness::Unavailable);
        assert_eq!(e, EmotionColor::Neutral);
    }

    #[test]
    fn hex_round_trip() {
        // Sanity: each color's hex matches what rgb() decomposes.
        for c in [
            EmotionColor::Anger,
            EmotionColor::Disgust,
            EmotionColor::Surprise,
            EmotionColor::Joy,
            EmotionColor::Interest,
            EmotionColor::Sadness,
            EmotionColor::Fear,
            EmotionColor::Neutral,
            EmotionColor::Inactive,
        ] {
            let (r, g, b) = c.rgb();
            let reconstructed = (r as u32) << 16 | (g as u32) << 8 | b as u32;
            assert_eq!(reconstructed, c.hex());
        }
    }
}
