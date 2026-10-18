//! An outline.
//!
//! A board is a Rust function that returns a `Board`. This one is only its
//! edge, a 30 × 20 mm rectangle on Edge.Cuts, and a title in silkscreen.
//! Try changing the corners or the title; the drawing follows as you type.

use bb_sdk::*;

fn build() -> Board {
    Board::new(Project::new("indicator")).items(items![
        outline(Layer::EdgeCuts, [at(20, 20), at(50, 20), at(50, 40), at(20, 40)], 0.1),
        text("INDICATORS", (37, 22.3)).size(1.2),
    ])
}

bb_sdk::export_board!(build);
