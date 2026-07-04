//! Puzzles: forced-win problems, exactly tsumego. "⊤ in 2" means the ⊤ side,
//! moving first, forces the collapse within two of its own assignments —
//! and the first move is unique, or it isn't a puzzle.

use crate::formula::F;
use crate::game::Side;
use crate::gen::{gen_formula, GenParams};
use crate::rng::Rng;
use crate::solve::Solver;

#[derive(Clone, Debug)]
pub struct Puzzle {
    pub title: String,
    pub f: F,
    /// The side you play; you always move first.
    pub you: Side,
    /// Number of *your* assignments in the forced win.
    pub mate_in: u32,
}

impl Puzzle {
    /// Total plies of the forced line: your moves interleaved with replies.
    pub fn plies(&self) -> u32 {
        2 * self.mate_in - 1
    }
}

/// Check the tsumego contract: `you` to move wins in exactly `mate_in` of
/// your assignments, with a unique winning first move.
pub fn is_sound(p: &Puzzle) -> bool {
    let mut s = Solver::new();
    let e = s.solve(&p.f, p.you);
    e.winner == p.you && e.plies == p.plies() && s.winning_moves(&p.f, p.you).len() == 1
}

/// Hunt for a sound puzzle with the given shape. Deterministic in `rng`.
pub fn find_puzzle(rng: &mut Rng, params: &GenParams, you: Side, mate_in: u32) -> Option<Puzzle> {
    for _ in 0..6000 {
        let f = gen_formula(rng, params);
        if crate::formula::glyph_count(&f) > params.max_glyphs || f.as_const().is_some() {
            continue;
        }
        let candidate = Puzzle {
            title: String::new(),
            f,
            you,
            mate_in,
        };
        if is_sound(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// The built-in book. First entry is the worked example from the design doc;
/// the rest are dealt from fixed seeds and verified sound by the test suite.
pub fn builtin_puzzles() -> Vec<Puzzle> {
    let mut out = Vec::new();

    // GAME.md's "mate in 2": (p ⇒ q) ∧ (p ∨ r) ∧ (r ⇒ q), ⊤ to move.
    let p = || F::Atom(0);
    let q = || F::Atom(1);
    let r = || F::Atom(2);
    out.push(Puzzle {
        title: "⊤ in 2 · the fork".into(),
        f: F::and(F::and(F::imp(p(), q()), F::or(p(), r())), F::imp(r(), q())),
        you: Side::Top,
        mate_in: 2,
    });

    // Seeded hunts. Each tuple: (seed, difficulty, side, mate_in, name).
    let specs: &[(u64, u8, Side, u32, &str)] = &[
        (0xA11CE, 1, Side::Bot, 2, "⊥ in 2 · first cut"),
        (0xB0B1, 2, Side::Top, 2, "⊤ in 2 · tempo"),
        (0xB0B2, 2, Side::Bot, 2, "⊥ in 2 · poisoned pair"),
        (0xC0DE1, 3, Side::Top, 2, "⊤ in 2 · antecedent trap"),
        (0xC0DE2, 3, Side::Bot, 3, "⊥ in 3 · the drag"),
        (0xD00D1, 3, Side::Top, 3, "⊤ in 3 · double threat"),
        (0xD00D2, 4, Side::Bot, 3, "⊥ in 3 · burn the bridge"),
        (0xE55E1, 4, Side::Top, 3, "⊤ in 3 · zugzwang"),
        (0xE55E2, 5, Side::Top, 3, "⊤ in 3 · the long read"),
        (0xF17A1, 5, Side::Bot, 3, "⊥ in 3 · endgame"),
    ];
    for &(seed, lvl, side, mate, name) in specs {
        let mut rng = Rng::new(seed);
        let params = GenParams::difficulty(lvl);
        if let Some(mut pz) = find_puzzle(&mut rng, &params, side, mate) {
            pz.title = name.into();
            out.push(pz);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula::pretty;
    use crate::game::Move;

    #[test]
    fn every_builtin_puzzle_is_sound() {
        let book = builtin_puzzles();
        assert!(book.len() >= 8, "book too thin: {} puzzles", book.len());
        for pz in &book {
            assert!(
                is_sound(pz),
                "unsound puzzle {:?}: {}",
                pz.title,
                pretty(&pz.f)
            );
        }
    }

    #[test]
    fn first_builtin_is_the_worked_example() {
        let book = builtin_puzzles();
        assert_eq!(pretty(&book[0].f), "(p ⇒ q) ∧ (p ∨ r) ∧ (r ⇒ q)");
        let mut s = Solver::new();
        assert_eq!(
            s.winning_moves(&book[0].f, Side::Top),
            vec![Move::new(1, true)]
        );
    }

    #[test]
    fn builtin_book_is_deterministic() {
        let a = builtin_puzzles();
        let b = builtin_puzzles();
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.f, y.f);
        }
    }
}
