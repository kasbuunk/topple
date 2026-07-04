//! The rewrite laws — the only physics the board obeys.
//!
//! After each assignment the board rewrites itself one law at a time. The
//! scheduler is deterministic: scan the tree in pre-order (outermost first,
//! left before right) and fire the first law that matches. Every law strictly
//! shrinks the tree, so the cascade always terminates; a formula with no
//! atoms always ends as a lone constant.

use crate::formula::{Path, F};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Law {
    TopAnd, // ⊤ ∧ x = x
    AndTop, // x ∧ ⊤ = x
    BotAnd, // ⊥ ∧ x = ⊥
    AndBot, // x ∧ ⊥ = ⊥
    TopOr,  // ⊤ ∨ x = ⊤
    OrTop,  // x ∨ ⊤ = ⊤
    BotOr,  // ⊥ ∨ x = x
    OrBot,  // x ∨ ⊥ = x
    TopImp, // ⊤ ⇒ x = x
    BotImp, // ⊥ ⇒ x = ⊤
    ImpTop, // x ⇒ ⊤ = ⊤
    ImpBot, // x ⇒ ⊥ = ¬x
    TopEq,  // (⊤ = x) = x
    EqTop,  // (x = ⊤) = x
    BotEq,  // (⊥ = x) = ¬x
    EqBot,  // (x = ⊥) = ¬x
    NotTop, // ¬⊤ = ⊥
    NotBot, // ¬⊥ = ⊤
    NotNot, // ¬¬x = x
}

impl Law {
    /// The equation, exactly as the toast displays it.
    pub fn equation(self) -> &'static str {
        match self {
            Law::TopAnd => "⊤ ∧ x  =  x",
            Law::AndTop => "x ∧ ⊤  =  x",
            Law::BotAnd => "⊥ ∧ x  =  ⊥",
            Law::AndBot => "x ∧ ⊥  =  ⊥",
            Law::TopOr => "⊤ ∨ x  =  ⊤",
            Law::OrTop => "x ∨ ⊤  =  ⊤",
            Law::BotOr => "⊥ ∨ x  =  x",
            Law::OrBot => "x ∨ ⊥  =  x",
            Law::TopImp => "⊤ ⇒ x  =  x",
            Law::BotImp => "⊥ ⇒ x  =  ⊤",
            Law::ImpTop => "x ⇒ ⊤  =  ⊤",
            Law::ImpBot => "x ⇒ ⊥  =  ¬x",
            Law::TopEq => "(⊤ = x)  =  x",
            Law::EqTop => "(x = ⊤)  =  x",
            Law::BotEq => "(⊥ = x)  =  ¬x",
            Law::EqBot => "(x = ⊥)  =  ¬x",
            Law::NotTop => "¬⊤  =  ⊥",
            Law::NotBot => "¬⊥  =  ⊤",
            Law::NotNot => "¬¬x  =  x",
        }
    }
}

/// Try every law at this single node. Constant-on-the-left laws take
/// priority, matching reading order; the result is confluent regardless.
fn match_node(f: &F) -> Option<(Law, F)> {
    use crate::formula::Op::*;
    match f {
        F::Not(x) => match &**x {
            F::Const(true) => Some((Law::NotTop, F::Const(false))),
            F::Const(false) => Some((Law::NotBot, F::Const(true))),
            F::Not(y) => Some((Law::NotNot, (**y).clone())),
            _ => None,
        },
        F::Bin(op, l, r) => {
            let lc = l.as_const();
            let rc = r.as_const();
            match op {
                And => match (lc, rc) {
                    (Some(true), _) => Some((Law::TopAnd, (**r).clone())),
                    (Some(false), _) => Some((Law::BotAnd, F::Const(false))),
                    (_, Some(true)) => Some((Law::AndTop, (**l).clone())),
                    (_, Some(false)) => Some((Law::AndBot, F::Const(false))),
                    _ => None,
                },
                Or => match (lc, rc) {
                    (Some(true), _) => Some((Law::TopOr, F::Const(true))),
                    (Some(false), _) => Some((Law::BotOr, (**r).clone())),
                    (_, Some(true)) => Some((Law::OrTop, F::Const(true))),
                    (_, Some(false)) => Some((Law::OrBot, (**l).clone())),
                    _ => None,
                },
                Imp => match (lc, rc) {
                    (Some(true), _) => Some((Law::TopImp, (**r).clone())),
                    (Some(false), _) => Some((Law::BotImp, F::Const(true))),
                    (_, Some(true)) => Some((Law::ImpTop, F::Const(true))),
                    (_, Some(false)) => Some((Law::ImpBot, F::not((**l).clone()))),
                    _ => None,
                },
                Eq => match (lc, rc) {
                    (Some(true), _) => Some((Law::TopEq, (**r).clone())),
                    (Some(false), _) => Some((Law::BotEq, F::not((**r).clone()))),
                    (_, Some(true)) => Some((Law::EqTop, (**l).clone())),
                    (_, Some(false)) => Some((Law::EqBot, F::not((**l).clone()))),
                    _ => None,
                },
            }
        }
        _ => None,
    }
}

/// One rewrite step: the law that fired, where, and the whole-board result.
#[derive(Clone, Debug)]
pub struct Step {
    pub law: Law,
    /// Path of the redex in the formula *before* this step.
    pub path: Path,
    /// The whole formula after this step.
    pub after: F,
}

/// Fire the first matching law (pre-order). None = the board is calm.
pub fn step(f: &F) -> Option<Step> {
    fn go(f: &F, path: &mut Path) -> Option<(Path, Law, F)> {
        if let Some((law, repl)) = match_node(f) {
            return Some((path.clone(), law, repl));
        }
        match f {
            F::Not(x) => {
                path.push(0);
                let r = go(x, path);
                path.pop();
                r
            }
            F::Bin(_, l, r) => {
                path.push(0);
                if let Some(hit) = go(l, path) {
                    path.pop();
                    return Some(hit);
                }
                path.pop();
                path.push(1);
                let hit = go(r, path);
                path.pop();
                hit
            }
            _ => None,
        }
    }
    let (path, law, repl) = go(f, &mut Vec::new())?;
    Some(Step {
        law,
        path: path.clone(),
        after: replace_at(f, &path, repl),
    })
}

fn replace_at(f: &F, path: &[u8], repl: F) -> F {
    if path.is_empty() {
        return repl;
    }
    match f {
        F::Not(x) => F::not(replace_at(x, &path[1..], repl)),
        F::Bin(op, l, r) => {
            if path[0] == 0 {
                F::bin(*op, replace_at(l, &path[1..], repl), (**r).clone())
            } else {
                F::bin(*op, (**l).clone(), replace_at(r, &path[1..], repl))
            }
        }
        _ => unreachable!("path leads through a leaf"),
    }
}

/// Run the cascade to quiescence, recording every step.
pub fn cascade(f: &F) -> (F, Vec<Step>) {
    let mut cur = f.clone();
    let mut steps = Vec::new();
    while let Some(s) = step(&cur) {
        cur = s.after.clone();
        steps.push(s);
    }
    (cur, steps)
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
    fn t() -> F {
        F::Const(true)
    }
    fn b() -> F {
        F::Const(false)
    }

    #[test]
    fn every_law_fires_and_rewrites_correctly() {
        let cases: Vec<(F, Law, &str)> = vec![
            (F::and(t(), p()), Law::TopAnd, "p"),
            (F::and(p(), t()), Law::AndTop, "p"),
            (F::and(b(), p()), Law::BotAnd, "⊥"),
            (F::and(p(), b()), Law::AndBot, "⊥"),
            (F::or(t(), p()), Law::TopOr, "⊤"),
            (F::or(p(), t()), Law::OrTop, "⊤"),
            (F::or(b(), p()), Law::BotOr, "p"),
            (F::or(p(), b()), Law::OrBot, "p"),
            (F::imp(t(), p()), Law::TopImp, "p"),
            (F::imp(b(), p()), Law::BotImp, "⊤"),
            (F::imp(p(), t()), Law::ImpTop, "⊤"),
            (F::imp(p(), b()), Law::ImpBot, "¬p"),
            (F::eq(t(), p()), Law::TopEq, "p"),
            (F::eq(p(), t()), Law::EqTop, "p"),
            (F::eq(b(), p()), Law::BotEq, "¬p"),
            (F::eq(p(), b()), Law::EqBot, "¬p"),
            (F::not(t()), Law::NotTop, "⊥"),
            (F::not(b()), Law::NotBot, "⊤"),
            (F::not(F::not(p())), Law::NotNot, "p"),
        ];
        for (f, law, want) in cases {
            let s = step(&f).unwrap_or_else(|| panic!("no law fired on {}", pretty(&f)));
            assert_eq!(s.law, law, "wrong law on {}", pretty(&f));
            assert_eq!(pretty(&s.after), want, "wrong result on {}", pretty(&f));
            assert!(s.path.is_empty());
        }
    }

    #[test]
    fn every_law_strictly_shrinks() {
        // The termination argument, checked mechanically on the cases above.
        let redexes = vec![
            F::and(t(), p()),
            F::and(p(), b()),
            F::or(t(), F::and(p(), q())),
            F::imp(F::or(p(), q()), b()),
            F::eq(b(), F::not(p())),
            F::not(F::not(F::and(p(), q()))),
        ];
        for f in redexes {
            let s = step(&f).unwrap();
            assert!(s.after.size() < f.size(), "{} did not shrink", pretty(&f));
        }
    }

    #[test]
    fn cascade_order_matches_worked_example_blunder() {
        // GAME.md: after p ≔ ⊤ in (p ⇒ q) ∧ (p ∨ r) ∧ (r ⇒ q):
        //   ⊤⇒x = x   →  q ∧ (⊤ ∨ r) ∧ (r ⇒ q)
        //   ⊤∨x = ⊤   →  q ∧ ⊤ ∧ (r ⇒ q)
        //   x∧⊤ = x   →  q ∧ (r ⇒ q)
        let f =
            F::and(F::and(F::imp(p(), q()), F::or(p(), r())), F::imp(r(), q())).substitute(0, true);
        let (end, steps) = cascade(&f);
        let trace: Vec<(Law, String)> = steps.iter().map(|s| (s.law, pretty(&s.after))).collect();
        assert_eq!(
            trace,
            vec![
                (Law::TopImp, "q ∧ (⊤ ∨ r) ∧ (r ⇒ q)".to_string()),
                (Law::TopOr, "q ∧ ⊤ ∧ (r ⇒ q)".to_string()),
                (Law::AndTop, "q ∧ (r ⇒ q)".to_string()),
            ]
        );
        assert_eq!(pretty(&end), "q ∧ (r ⇒ q)");
    }

    #[test]
    fn cascade_order_matches_worked_example_winning_move() {
        // q ≔ ⊤: x⇒⊤ = ⊤ (twice), then the ∧-laws leave p ∨ r.
        let f =
            F::and(F::and(F::imp(p(), q()), F::or(p(), r())), F::imp(r(), q())).substitute(1, true);
        let (end, _) = cascade(&f);
        assert_eq!(pretty(&end), "p ∨ r");
    }

    #[test]
    fn closed_formulas_always_collapse_to_a_constant() {
        // Exhaustive over all shapes up to depth 3 with constants only.
        fn shapes(depth: usize) -> Vec<F> {
            if depth == 0 {
                return vec![F::Const(true), F::Const(false)];
            }
            let subs = shapes(depth - 1);
            let mut out = subs.clone();
            for l in &subs {
                out.push(F::not(l.clone()));
                for r in &subs {
                    for op in [Op::And, Op::Or, Op::Imp, Op::Eq] {
                        out.push(F::bin(op, l.clone(), r.clone()));
                    }
                }
            }
            out
        }
        for f in shapes(2) {
            let (end, _) = cascade(&f);
            let want = f.eval(0);
            assert_eq!(
                end,
                F::Const(want),
                "{} should collapse to {}",
                pretty(&f),
                want
            );
        }
    }

    #[test]
    fn cascade_preserves_truth_value() {
        // Rewriting is equational: the cascade result must be logically
        // equivalent to the input under every assignment of remaining atoms.
        let f = F::and(
            F::imp(F::or(p(), q()), F::not(r())),
            F::eq(q(), F::imp(r(), p())),
        );
        for atom in 0..3u8 {
            for val in [true, false] {
                let g = f.substitute(atom, val);
                let (end, _) = cascade(&g);
                for env in 0..8u8 {
                    assert_eq!(g.eval(env), end.eval(env));
                }
            }
        }
    }
}
