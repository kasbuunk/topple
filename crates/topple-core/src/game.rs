//! Game rules: sides, moves, the win condition.

use crate::formula::{Atom, F};
use crate::laws::{cascade, Step};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    Top,
    Bot,
}

impl Side {
    pub fn constant(self) -> bool {
        matches!(self, Side::Top)
    }
    pub fn of_constant(b: bool) -> Side {
        if b {
            Side::Top
        } else {
            Side::Bot
        }
    }
    pub fn other(self) -> Side {
        match self {
            Side::Top => Side::Bot,
            Side::Bot => Side::Top,
        }
    }
    pub fn glyph(self) -> char {
        match self {
            Side::Top => '⊤',
            Side::Bot => '⊥',
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Move {
    pub atom: Atom,
    pub value: bool,
}

impl Move {
    pub fn new(atom: Atom, value: bool) -> Move {
        Move { atom, value }
    }
}

/// The winner, if the board has collapsed.
pub fn winner(f: &F) -> Option<Side> {
    f.as_const().map(Side::of_constant)
}

/// All legal moves: any atom still on the board, either value.
pub fn legal_moves(f: &F) -> Vec<Move> {
    let mut out = Vec::new();
    for a in f.atoms() {
        out.push(Move::new(a, true));
        out.push(Move::new(a, false));
    }
    out
}

/// Apply a move: substitute every occurrence, then run the cascade.
/// Returns the final board and the full step trace for animation.
/// Errors if the atom is no longer on the board.
pub fn apply_move(f: &F, mv: Move) -> Result<(F, Vec<Step>), &'static str> {
    if f.count_atom(mv.atom) == 0 {
        return Err("atom is not on the board");
    }
    let substituted = f.substitute(mv.atom, mv.value);
    let (end, steps) = cascade(&substituted);
    Ok((end, steps))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula::pretty;

    fn worked_example() -> F {
        let p = || F::Atom(0);
        let q = || F::Atom(1);
        let r = || F::Atom(2);
        F::and(F::and(F::imp(p(), q()), F::or(p(), r())), F::imp(r(), q()))
    }

    #[test]
    fn game_ends_when_board_is_a_lone_constant() {
        let f = worked_example();
        assert_eq!(winner(&f), None);
        // The full blunder line from GAME.md: p ≔ ⊤, then q ≔ ⊥ ends it.
        let (f, _) = apply_move(&f, Move::new(0, true)).unwrap();
        assert_eq!(pretty(&f), "q ∧ (r ⇒ q)");
        let (f, _) = apply_move(&f, Move::new(1, false)).unwrap();
        assert_eq!(winner(&f), Some(Side::Bot));
        // r was deleted from the board unassigned; no legal moves remain.
        assert!(legal_moves(&f).is_empty());
    }

    #[test]
    fn deleted_atoms_cannot_be_assigned() {
        let f = worked_example();
        let (f, _) = apply_move(&f, Move::new(1, true)).unwrap(); // q ≔ ⊤ → p ∨ r
        let (f, _) = apply_move(&f, Move::new(0, false)).unwrap(); // p ≔ ⊥ → r
        assert_eq!(pretty(&f), "r");
        assert!(apply_move(&f, Move::new(0, true)).is_err());
        assert_eq!(
            legal_moves(&f),
            vec![Move::new(2, true), Move::new(2, false)]
        );
    }

    #[test]
    fn every_move_strictly_reduces_occurrences() {
        let f = worked_example();
        for mv in legal_moves(&f) {
            let (g, _) = apply_move(&f, mv).unwrap();
            assert!(g.total_occurrences() < f.total_occurrences());
        }
    }
}
