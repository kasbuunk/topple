//! Perfect play. At board sizes within the legibility budget (≤ 8 atoms) the
//! whole game tree fits in a memo table, so the Adversary never blunders and
//! the generator can price every formula it deals.

use crate::formula::F;
use crate::game::{apply_move, legal_moves, winner, Move, Side};
use std::collections::HashMap;

/// Value of a position from the point of view of the game itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Eval {
    pub winner: Side,
    /// Plies until the board collapses under optimal play: the winner hurries,
    /// the loser drags. 0 when the board is already a constant.
    pub plies: u32,
}

pub struct Solver {
    memo: HashMap<(String, Side), Eval>,
}

impl Default for Solver {
    fn default() -> Self {
        Self::new()
    }
}

impl Solver {
    pub fn new() -> Solver {
        Solver {
            memo: HashMap::new(),
        }
    }

    /// Solve with `to_move` choosing the next assignment.
    pub fn solve(&mut self, f: &F, to_move: Side) -> Eval {
        if let Some(w) = winner(f) {
            return Eval {
                winner: w,
                plies: 0,
            };
        }
        let key = (f.key(), to_move);
        if let Some(e) = self.memo.get(&key) {
            return e.clone();
        }
        let mut best: Option<Eval> = None;
        for mv in legal_moves(f) {
            let (g, _) = apply_move(f, mv).expect("legal move");
            let sub = self.solve(&g, to_move.other());
            let cand = Eval {
                winner: sub.winner,
                plies: sub.plies + 1,
            };
            best = Some(match best {
                None => cand,
                Some(b) => pick(to_move, b, cand),
            });
        }
        let e = best.expect("non-constant formula has a legal move");
        self.memo.insert(key, e.clone());
        e
    }

    /// Moves that preserve the best achievable outcome for `to_move`
    /// (and among those, the best ply count for them).
    pub fn best_moves(&mut self, f: &F, to_move: Side) -> Vec<Move> {
        let target = self.solve(f, to_move);
        let mut out = Vec::new();
        for mv in legal_moves(f) {
            let (g, _) = apply_move(f, mv).expect("legal move");
            let sub = self.solve(&g, to_move.other());
            if sub.winner == target.winner && sub.plies + 1 == target.plies {
                out.push(mv);
            }
        }
        out
    }

    /// Moves that win for `to_move` (regardless of speed). Empty iff lost.
    pub fn winning_moves(&mut self, f: &F, to_move: Side) -> Vec<Move> {
        let mut out = Vec::new();
        for mv in legal_moves(f) {
            let (g, _) = apply_move(f, mv).expect("legal move");
            if self.solve(&g, to_move.other()).winner == to_move {
                out.push(mv);
            }
        }
        out
    }
}

/// The player to move prefers winning, then (if winning) fewer plies,
/// then (if losing) more plies.
fn pick(to_move: Side, a: Eval, b: Eval) -> Eval {
    let a_wins = a.winner == to_move;
    let b_wins = b.winner == to_move;
    match (a_wins, b_wins) {
        (true, false) => a,
        (false, true) => b,
        (true, true) => {
            if b.plies < a.plies {
                b
            } else {
                a
            }
        }
        (false, false) => {
            if b.plies > a.plies {
                b
            } else {
                a
            }
        }
    }
}

/// Winner for each choice of who moves first: `(if Top moves first, if Bot
/// moves first)`. This is the whole pie-rule pricing of a fresh formula.
pub fn solve_both_orders(f: &F) -> (Side, Side) {
    let mut s = Solver::new();
    (s.solve(f, Side::Top).winner, s.solve(f, Side::Bot).winner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula::{pretty, Op};

    fn p() -> F {
        F::Atom(0)
    }
    fn q() -> F {
        F::Atom(1)
    }
    fn r() -> F {
        F::Atom(2)
    }

    fn worked_example() -> F {
        F::and(F::and(F::imp(p(), q()), F::or(p(), r())), F::imp(r(), q()))
    }

    #[test]
    fn worked_example_is_top_win_in_two_with_unique_key_move() {
        // GAME.md: ⊤ to move wins; q ≔ ⊤ is the only winning assignment,
        // and it is a forced win in two of ⊤'s moves (3 plies).
        let f = worked_example();
        let mut s = Solver::new();
        let e = s.solve(&f, Side::Top);
        assert_eq!(e.winner, Side::Top);
        assert_eq!(e.plies, 3, "mate in 2 (⊤ moves 1 and 3)");
        let wins = s.winning_moves(&f, Side::Top);
        assert_eq!(wins, vec![Move::new(1, true)], "only q ≔ ⊤ wins");
    }

    #[test]
    fn worked_example_follow_up_fork_works() {
        // After q ≔ ⊤ the board is p ∨ r with ⊥ to move: ⊥ loses both prongs.
        let (g, _) = apply_move(&worked_example(), Move::new(1, true)).unwrap();
        assert_eq!(pretty(&g), "p ∨ r");
        let mut s = Solver::new();
        assert_eq!(s.solve(&g, Side::Bot).winner, Side::Top);
        // And any ⊤-assignment by ⊥ is instant suicide via ⊤∨x = ⊤.
        for atom in [0, 2] {
            let (h, _) = apply_move(&g, Move::new(atom, true)).unwrap();
            assert_eq!(winner(&h), Some(Side::Top));
        }
    }

    #[test]
    fn lone_atom_is_won_by_the_mover() {
        let mut s = Solver::new();
        assert_eq!(s.solve(&p(), Side::Top).winner, Side::Top);
        assert_eq!(s.solve(&p(), Side::Bot).winner, Side::Bot);
    }

    #[test]
    fn lone_equality_is_mutual_zugzwang() {
        // (p = q): whoever touches it first hands the opponent the deciding
        // assignment — the mover always loses.
        let f = F::eq(p(), q());
        let mut s = Solver::new();
        assert_eq!(s.solve(&f, Side::Top).winner, Side::Bot);
        assert_eq!(s.solve(&f, Side::Bot).winner, Side::Top);
    }

    #[test]
    fn conjunction_of_two_atoms_favours_bot_disjunction_top() {
        let mut s = Solver::new();
        let f = F::and(p(), q());
        // ⊥ just kills either atom regardless of who starts.
        assert_eq!(s.solve(&f, Side::Top).winner, Side::Bot);
        assert_eq!(s.solve(&f, Side::Bot).winner, Side::Bot);
        let g = F::or(p(), q());
        assert_eq!(s.solve(&g, Side::Top).winner, Side::Top);
        assert_eq!(s.solve(&g, Side::Bot).winner, Side::Top);
    }

    #[test]
    fn solver_agrees_with_brute_force_on_random_formulas() {
        // Cross-check the memoized solver against a memo-free reference
        // implementation on a swarm of small random formulas.
        use crate::rng::Rng;

        fn brute(f: &F, to_move: Side) -> Side {
            if let Some(w) = winner(f) {
                return w;
            }
            let mut can_win = false;
            for mv in legal_moves(f) {
                let (g, _) = apply_move(f, mv).unwrap();
                if brute(&g, to_move.other()) == to_move {
                    can_win = true;
                    break;
                }
            }
            if can_win {
                to_move
            } else {
                to_move.other()
            }
        }

        let mut rng = Rng::new(0xC0FFEE);
        for _ in 0..300 {
            let f = random_formula(&mut rng, 4);
            if f.total_occurrences() > 6 {
                continue;
            }
            let mut s = Solver::new();
            for side in [Side::Top, Side::Bot] {
                assert_eq!(
                    s.solve(&f, side).winner,
                    brute(&f, side),
                    "disagreement on {} ({:?} to move)",
                    pretty(&f),
                    side
                );
            }
        }
    }

    fn random_formula(rng: &mut crate::rng::Rng, depth: u32) -> F {
        if depth == 0 || rng.below(4) == 0 {
            return F::Atom(rng.below(3) as u8);
        }
        match rng.below(6) {
            0 => F::not(random_formula(rng, depth - 1)),
            1 => F::bin(
                Op::Imp,
                random_formula(rng, depth - 1),
                random_formula(rng, depth - 1),
            ),
            2 => F::bin(
                Op::Eq,
                random_formula(rng, depth - 1),
                random_formula(rng, depth - 1),
            ),
            3 | 4 => F::bin(
                Op::And,
                random_formula(rng, depth - 1),
                random_formula(rng, depth - 1),
            ),
            _ => F::bin(
                Op::Or,
                random_formula(rng, depth - 1),
                random_formula(rng, depth - 1),
            ),
        }
    }

    #[test]
    fn winner_always_has_a_move_that_stays_winning() {
        // Strategy consistency: from any winning position, best_moves is
        // non-empty and following it keeps the win.
        let f = worked_example();
        let mut s = Solver::new();
        let mut cur = f;
        let mut to_move = Side::Top;
        let expected = s.solve(&cur, to_move).winner;
        while winner(&cur).is_none() {
            let mv = s.best_moves(&cur, to_move)[0];
            let (next, _) = apply_move(&cur, mv).unwrap();
            cur = next;
            to_move = to_move.other();
            assert_eq!(
                winner(&cur).unwrap_or_else(|| s.solve(&cur, to_move).winner),
                expected
            );
        }
        assert_eq!(winner(&cur), Some(expected));
    }
}
