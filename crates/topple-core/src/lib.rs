//! ⊤OPP⊥E core: two players adversarially build a valuation, one atom per
//! turn, and the only physics the board obeys are the boolean laws.
//!
//! This crate is pure logic — no I/O, no clock, no platform. Everything is
//! deterministic so that a seed deals the same gauntlet on a Miyoo Mini, in
//! a browser, and on a laptop.

pub mod formula;
pub mod game;
pub mod gen;
pub mod laws;
pub mod puzzles;
pub mod rng;
pub mod solve;

pub use formula::{atom_name, glyph_count, pretty, span_of, tokens, Atom, Op, Path, Token, F};
pub use game::{apply_move, legal_moves, winner, Move, Side};
pub use gen::{
    deal, gauntlet, gauntlet_seed, parse_share_code, price, share_code, Deal, DealKind, GenParams,
    GAUNTLET_SIZE,
};
pub use laws::{cascade, step, Law, Step};
pub use puzzles::{builtin_puzzles, find_puzzle, is_sound, Puzzle};
pub use rng::{hash_str, Rng};
pub use solve::{solve_both_orders, Eval, Solver};
