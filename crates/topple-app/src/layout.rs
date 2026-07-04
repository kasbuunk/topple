//! Formula layout: core tokens → positioned glyphs, wrapped and centered,
//! with enough provenance to glow every occurrence of the hovered atom and
//! to box the redex a law is about to rewrite.

use crate::font::FontEngine;
use topple_core::{span_of, tokens, Atom, Path, Token, F};

#[derive(Clone, Debug)]
pub struct LaidGlyph {
    pub ch: char,
    pub x: f32,
    pub y_baseline: f32,
    pub w: f32,
    pub line: usize,
    pub tok_index: usize,
    pub atom: Option<Atom>,
    pub path: Path,
}

pub struct Layout {
    pub glyphs: Vec<LaidGlyph>,
    pub toks: Vec<Token>,
    pub size: f32,
    pub lines: usize,
    pub line_height: f32,
    /// Top y of the block.
    pub top: f32,
}

/// Candidate sizes: ≥32px per the design doc, shrinking only when a board
/// would not fit the panel otherwise.
const SIZES: [f32; 5] = [40.0, 36.0, 32.0, 26.0, 22.0];

pub fn layout_formula(
    fonts: &mut FontEngine,
    f: &F,
    zoom: Option<&Path>,
    max_width: f32,
    center_x: f32,
    center_y: f32,
    max_lines: usize,
) -> Layout {
    let all = tokens(f);
    // Zoomed view shows just the subtree's token span.
    let toks: Vec<Token> = match zoom.and_then(|p| span_of(&all, p)) {
        Some((s, e)) => all[s..e].to_vec(),
        None => all,
    };

    for (i, &size) in SIZES.iter().enumerate() {
        let lines = wrap(fonts, &toks, size, max_width);
        if lines.len() <= max_lines || i == SIZES.len() - 1 {
            return place(fonts, &toks, &lines, size, center_x, center_y);
        }
    }
    unreachable!()
}

/// Balanced wrap: split at the *shallowest* connective nearest the middle,
/// recursively, so clauses stay whole and lines stay even. The break lands
/// before the operator glyph, so continuation lines open with a connective —
/// the way logicians break formulas.
fn wrap(fonts: &mut FontEngine, toks: &[Token], size: f32, max_width: f32) -> Vec<(usize, usize)> {
    let mut lines = Vec::new();
    split(fonts, toks, 0, toks.len(), size, max_width, &mut lines, 0);
    lines
}

#[allow(clippy::too_many_arguments)]
fn split(
    fonts: &mut FontEngine,
    toks: &[Token],
    start: usize,
    end: usize,
    size: f32,
    max_width: f32,
    out: &mut Vec<(usize, usize)>,
    depth: usize,
) {
    let width: f32 = (start..end)
        .map(|i| fonts.char_advance(size, toks[i].ch))
        .sum();
    if width <= max_width || depth >= 4 {
        out.push((start, end));
        return;
    }
    // Candidates: a space directly before an operator glyph. Rank by the
    // operator node's depth (shallower first), then by distance from the
    // midpoint of this span.
    let mut x = 0.0;
    let mut best: Option<(usize, f32, usize)> = None; // (path depth, |x-mid|, idx)
    for i in start..end {
        let w = fonts.char_advance(size, toks[i].ch);
        if toks[i].space && i + 1 < end && !toks[i + 1].space && i > start {
            let next_is_op = matches!(toks[i + 1].ch, '∧' | '∨' | '⇒' | '=');
            if next_is_op {
                let d = toks[i + 1].path.len();
                let dist = (x + w - width / 2.0).abs();
                let better = match &best {
                    None => true,
                    Some((bd, bdist, _)) => (d, dist) < (*bd, *bdist),
                };
                if better {
                    best = Some((d, dist, i));
                }
            }
        }
        x += w;
    }
    match best {
        Some((_, _, cut)) => {
            split(fonts, toks, start, cut, size, max_width, out, depth + 1);
            split(fonts, toks, cut + 1, end, size, max_width, out, depth + 1);
        }
        None => out.push((start, end)),
    }
}

fn place(
    fonts: &mut FontEngine,
    toks: &[Token],
    lines: &[(usize, usize)],
    size: f32,
    center_x: f32,
    center_y: f32,
) -> Layout {
    let lh = fonts.line_height(size) * 1.15;
    let ascent = fonts.ascent(size);
    let block_h = lines.len() as f32 * lh;
    let top = center_y - block_h / 2.0;
    let mut glyphs = Vec::new();
    for (li, &(s, e)) in lines.iter().enumerate() {
        // Trim leading/trailing spaces on the line for clean centering.
        let mut s = s;
        let mut e = e;
        while s < e && toks[s].space {
            s += 1;
        }
        while e > s && toks[e - 1].space {
            e -= 1;
        }
        let width: f32 = (s..e).map(|i| fonts.char_advance(size, toks[i].ch)).sum();
        let mut x = center_x - width / 2.0;
        let baseline = top + li as f32 * lh + ascent;
        #[allow(clippy::needless_range_loop)]
        for i in s..e {
            let w = fonts.char_advance(size, toks[i].ch);
            glyphs.push(LaidGlyph {
                ch: toks[i].ch,
                x,
                y_baseline: baseline,
                w,
                line: li,
                tok_index: i,
                atom: toks[i].atom,
                path: toks[i].path.clone(),
            });
            x += w;
        }
    }
    Layout {
        glyphs,
        toks: toks.to_vec(),
        size,
        lines: lines.len(),
        line_height: lh,
        top,
    }
}

impl Layout {
    /// Indices (into `glyphs`) of atom occurrences, in reading order.
    pub fn occurrences(&self) -> Vec<usize> {
        self.glyphs
            .iter()
            .enumerate()
            .filter(|(_, g)| g.atom.is_some())
            .map(|(i, _)| i)
            .collect()
    }

    /// Glyph range covering the subtree at `path` (token-span → glyph indices).
    pub fn glyph_span_of_path(&self, path: &Path) -> Vec<usize> {
        self.glyphs
            .iter()
            .enumerate()
            .filter(|(_, g)| g.path.len() >= path.len() && g.path[..path.len()] == path[..])
            .map(|(i, _)| i)
            .collect()
    }
}
