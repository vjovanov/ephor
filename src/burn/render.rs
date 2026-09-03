//! How a burn number is spelled, for both surfaces at once (§FS-013-burn.7).
//!
//! Presentation, but shared presentation. The one thing the page and the
//! command must not disagree about is what an unknown price looks like: a
//! screen that drew `$0.00` where the command printed `unpriced` would be two
//! answers to one question, and the wrong one is the one that says something
//! was free.

/// A token count, short enough for a column.
pub fn tokens(count: u64) -> String {
    match count {
        0 => "-".to_string(),
        count if count < 1_000 => count.to_string(),
        count if count < 1_000_000 => format!("{:.1}k", count as f64 / 1_000.0),
        count if count < 1_000_000_000 => format!("{:.1}M", count as f64 / 1_000_000.0),
        count => format!("{:.1}G", count as f64 / 1_000_000_000.0),
    }
}

/// What a group cost, or that nobody knows (§FS-013-burn.7).
///
/// The zero case is the point: a priced nothing prints `$0.00` and an unknown
/// prints `unpriced`, and neither ever wears the other's spelling.
pub fn cost(usd: Option<f64>) -> String {
    match usd {
        Some(usd) => format!("${usd:.2}"),
        None => "unpriced".to_string(),
    }
}

/// A rate, per minute.
pub fn rate(per_minute: u64) -> String {
    format!("{}/min", tokens(per_minute))
}

/// The eight blocks a sparkline is drawn out of, scaled to its own tallest
/// bar — an absolute scale would draw every quiet window as a flat line and
/// every busy one as a wall.
pub fn spark(bars: &[u64]) -> String {
    const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let tallest = bars.iter().copied().max().unwrap_or(0);
    if tallest == 0 {
        return " ".repeat(bars.len());
    }
    bars.iter()
        .map(|bar| {
            if *bar == 0 {
                return ' ';
            }
            let step = (*bar as f64 / tallest as f64 * (BLOCKS.len() - 1) as f64).round() as usize;
            BLOCKS[step.min(BLOCKS.len() - 1)]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The distinction §FS-013-burn.7 exists for, at the one place both
    /// surfaces read it from.
    #[test]
    fn unpriced_never_wears_a_zeros_spelling() {
        assert_eq!(cost(None), "unpriced");
        assert_eq!(cost(Some(0.0)), "$0.00");
        assert_eq!(cost(Some(4.126)), "$4.13");
    }

    #[test]
    fn a_count_is_spelled_at_its_own_scale() {
        assert_eq!(tokens(0), "-");
        assert_eq!(tokens(412), "412");
        assert_eq!(tokens(41_000), "41.0k");
        assert_eq!(tokens(31_200_000), "31.2M");
    }

    /// A quiet span is blank rather than a floor of low blocks, so a reader
    /// can see where nothing ran.
    #[test]
    fn a_sparkline_scales_to_itself_and_leaves_its_gaps() {
        assert_eq!(spark(&[0, 0, 0]), "   ");
        let drawn = spark(&[0, 1, 100]);
        assert_eq!(drawn.chars().next(), Some(' '));
        assert_eq!(drawn.chars().last(), Some('█'));
    }
}
