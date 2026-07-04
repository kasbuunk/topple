//! The dealer. Random formulas are cheap; *tense* formulas — positions whose
//! outcome flips on best play — are what get served. Every candidate is
//! solved before it reaches the screen, so there are no dead boards.

use crate::formula::{glyph_count, Op, F};
use crate::game::{legal_moves, Side};
use crate::rng::{hash_str, Rng};
use crate::solve::Solver;

#[derive(Clone, Debug)]
pub struct GenParams {
    /// Distinct atoms (≤ 8, the legibility cap).
    pub atoms: u8,
    /// Extra duplicate occurrences beyond one per atom.
    pub extra_occ: u8,
    /// Weights for ∧ ∨ ⇒ =.
    pub ops: [u64; 4],
    /// Probability (‰) of negating a generated subformula.
    pub neg_permille: u64,
    /// Non-space glyph budget (design doc: ~30, hard ceiling for 640×480).
    pub max_glyphs: usize,
}

impl GenParams {
    /// Difficulty scales by atom count and operator mix — never by blunders.
    pub fn difficulty(level: u8) -> GenParams {
        match level.clamp(1, 5) {
            // Four atoms, not three: with ∧/∨ only, a side-dominant *and*
            // blunderable board needs two independent disjunctive threats,
            // which takes four atoms — e.g. (p ∨ q) ∧ (r ∨ s).
            1 => GenParams {
                atoms: 4,
                extra_occ: 0,
                ops: [4, 4, 0, 0],
                neg_permille: 0,
                max_glyphs: 26,
            },
            2 => GenParams {
                atoms: 4,
                extra_occ: 1,
                ops: [4, 4, 2, 0],
                neg_permille: 0,
                max_glyphs: 30,
            },
            3 => GenParams {
                atoms: 5,
                extra_occ: 2,
                ops: [4, 4, 3, 0],
                neg_permille: 40,
                max_glyphs: 32,
            },
            4 => GenParams {
                atoms: 6,
                extra_occ: 2,
                ops: [4, 4, 3, 1],
                neg_permille: 70,
                max_glyphs: 34,
            },
            _ => GenParams {
                atoms: 8,
                extra_occ: 1,
                ops: [3, 3, 3, 2],
                neg_permille: 90,
                max_glyphs: 34,
            },
        }
    }
}

/// What kind of tension the mode needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DealKind {
    /// One side wins no matter who moves first (but only with correct play).
    /// Served when the human's pie-rule role is *picking the side*.
    SideDominant,
    /// The winner depends on who moves first.
    /// Served when the human's pie-rule role is *picking the tempo*.
    TempoSensitive,
    /// Anything tense.
    Any,
}

/// A priced formula, ready to be played.
#[derive(Clone, Debug)]
pub struct Deal {
    pub f: F,
    /// Winner if ⊤ assigns first.
    pub if_top_first: Side,
    /// Winner if ⊥ assigns first.
    pub if_bot_first: Side,
    /// Optimal game length (plies), maximised over the two orders.
    pub plies: u32,
}

impl Deal {
    pub fn side_dominant(&self) -> Option<Side> {
        (self.if_top_first == self.if_bot_first).then_some(self.if_top_first)
    }
}

/// Build a random tree over a shuffled bag of atom occurrences.
pub fn gen_formula(rng: &mut Rng, params: &GenParams) -> F {
    let mut leaves: Vec<u8> = (0..params.atoms).collect();
    for _ in 0..params.extra_occ {
        leaves.push(rng.below(params.atoms as u64) as u8);
    }
    rng.shuffle(&mut leaves);
    build(rng, params, &leaves)
}

fn build(rng: &mut Rng, params: &GenParams, leaves: &[u8]) -> F {
    let f = if leaves.len() == 1 {
        F::Atom(leaves[0])
    } else {
        // Split point biased toward the middle for readable, bushy trees.
        let n = leaves.len();
        let cut = if n == 2 {
            1
        } else {
            1 + ((rng.below((n - 1) as u64) + rng.below((n - 1) as u64)) / 2) as usize
        };
        let op = match rng.pick_weighted(&params.ops) {
            0 => Op::And,
            1 => Op::Or,
            2 => Op::Imp,
            _ => Op::Eq,
        };
        F::bin(
            op,
            build(rng, params, &leaves[..cut]),
            build(rng, params, &leaves[cut..]),
        )
    };
    // Negate leaves and small groups occasionally; never stack ¬¬.
    if params.neg_permille > 0 && !matches!(f, F::Not(_)) && rng.chance(params.neg_permille) {
        F::not(f)
    } else {
        f
    }
}

/// Solve and price a formula.
pub fn price(f: &F) -> Deal {
    let mut s = Solver::new();
    let top = s.solve(f, Side::Top);
    let bot = s.solve(f, Side::Bot);
    Deal {
        f: f.clone(),
        if_top_first: top.winner,
        if_bot_first: bot.winner,
        plies: top.plies.max(bot.plies),
    }
}

/// Is this deal worth a player's time?
fn is_tense(deal: &Deal, kind: DealKind) -> bool {
    // Never a dead board, never an instant kill.
    if deal.plies < 3 {
        return false;
    }
    let mut s = Solver::new();
    match kind {
        DealKind::SideDominant => {
            let Some(w) = deal.side_dominant() else {
                return false;
            };
            // The dominant side must be able to throw the game away under
            // both orders — otherwise the round plays itself.
            for first in [Side::Top, Side::Bot] {
                let total = legal_moves(&deal.f).len();
                let winning = s.winning_moves(&deal.f, first).len();
                if first == w && winning >= total {
                    return false;
                }
            }
            true
        }
        DealKind::TempoSensitive => deal.if_top_first != deal.if_bot_first,
        DealKind::Any => {
            // Tense enough: someone to move must have a real choice.
            let total = legal_moves(&deal.f).len();
            let w_top = s.winning_moves(&deal.f, Side::Top).len();
            let w_bot = s.winning_moves(&deal.f, Side::Bot).len();
            w_top < total || w_bot < total
        }
    }
}

/// Deal a tense formula. Deterministic in `rng`; always returns.
pub fn deal(rng: &mut Rng, params: &GenParams, kind: DealKind) -> Deal {
    let mut fallback: Option<Deal> = None;
    for _ in 0..4000 {
        let f = gen_formula(rng, params);
        if glyph_count(&f) > params.max_glyphs || f.as_const().is_some() {
            continue;
        }
        let d = price(&f);
        if is_tense(&d, kind) {
            return d;
        }
        if d.plies >= 2 && fallback.is_none() {
            fallback = Some(d);
        }
    }
    // Practically unreachable; a merely-playable board beats a panic.
    fallback.unwrap_or_else(|| price(&F::or(F::and(F::Atom(0), F::Atom(1)), F::Atom(2))))
}

/// The Daily Gauntlet: five deals, difficulty ramping 1→5, from a date.
pub const GAUNTLET_SIZE: usize = 5;

pub fn gauntlet_seed(date_iso: &str) -> u32 {
    (hash_str(date_iso) ^ 0x7099_1e5a) as u32
}

pub fn gauntlet(seed: u32) -> Vec<Deal> {
    let mut rng = Rng::new(0x7099_1e00_0000_0000 ^ seed as u64);
    (1..=GAUNTLET_SIZE as u8)
        .map(|lvl| {
            let kind = if lvl % 2 == 0 {
                DealKind::TempoSensitive
            } else {
                DealKind::SideDominant
            };
            deal(&mut rng, &GenParams::difficulty(lvl), kind)
        })
        .collect()
}

/// Share codes: `TPL-XXXXXXX` (Crockford base32, 7 chars = 35 bits ⊇ u32).
const B32: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

pub fn share_code(seed: u32) -> String {
    let mut s = String::from("TPL-");
    let mut v = seed as u64;
    let mut chars = [0u8; 7];
    for c in chars.iter_mut().rev() {
        *c = B32[(v & 31) as usize];
        v >>= 5;
    }
    s.extend(chars.iter().map(|&c| c as char));
    s
}

pub fn parse_share_code(code: &str) -> Option<u32> {
    let raw = code.trim().to_ascii_uppercase();
    let raw = raw.strip_prefix("TPL-").unwrap_or(&raw);
    if raw.len() != 7 {
        return None;
    }
    let mut v: u64 = 0;
    for ch in raw.bytes() {
        // Crockford: I/L read as 1, O as 0.
        let ch = match ch {
            b'I' | b'L' => b'1',
            b'O' => b'0',
            c => c,
        };
        let d = B32.iter().position(|&b| b == ch)? as u64;
        v = (v << 5) | d;
    }
    u32::try_from(v).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formula::pretty;

    #[test]
    fn generator_is_deterministic() {
        let p = GenParams::difficulty(3);
        let a: Vec<String> = {
            let mut rng = Rng::new(99);
            (0..5)
                .map(|_| pretty(&deal(&mut rng, &p, DealKind::Any).f))
                .collect()
        };
        let b: Vec<String> = {
            let mut rng = Rng::new(99);
            (0..5)
                .map(|_| pretty(&deal(&mut rng, &p, DealKind::Any).f))
                .collect()
        };
        assert_eq!(a, b);
    }

    #[test]
    fn deals_respect_the_legibility_budget() {
        for lvl in 1..=5 {
            let p = GenParams::difficulty(lvl);
            let mut rng = Rng::new(lvl as u64 * 17);
            for _ in 0..10 {
                let d = deal(&mut rng, &p, DealKind::Any);
                assert!(glyph_count(&d.f) <= p.max_glyphs);
                assert!(d.f.atoms().len() <= 8);
                assert!(d.plies >= 2, "dead board dealt: {}", pretty(&d.f));
            }
        }
    }

    #[test]
    fn side_dominant_deals_are_dominant_but_blunderable() {
        // The pick-a-side pie-rule rounds depend on this contract at every
        // difficulty: a dominant side exists, the game lasts, and the
        // dominant side can throw it away.
        for lvl in 1..=5 {
            let p = GenParams::difficulty(lvl);
            let mut rng = Rng::new(1234 + lvl as u64);
            for _ in 0..4 {
                let d = deal(&mut rng, &p, DealKind::SideDominant);
                let w = d
                    .side_dominant()
                    .unwrap_or_else(|| panic!("lv{lvl}: not dominant: {}", pretty(&d.f)));
                let mut s = Solver::new();
                assert_eq!(s.solve(&d.f, Side::Top).winner, w);
                assert_eq!(s.solve(&d.f, Side::Bot).winner, w);
                assert!(d.plies >= 3, "lv{lvl}: too short: {}", pretty(&d.f));
                // Winning side moving first must have at least one losing move.
                let total = legal_moves(&d.f).len();
                assert!(
                    s.winning_moves(&d.f, w).len() < total,
                    "lv{lvl}: unloseable: {}",
                    pretty(&d.f)
                );
            }
        }
    }

    #[test]
    fn tempo_sensitive_deals_flip_on_order() {
        let p = GenParams::difficulty(2);
        let mut rng = Rng::new(555);
        for _ in 0..8 {
            let d = deal(&mut rng, &p, DealKind::TempoSensitive);
            assert_ne!(d.if_top_first, d.if_bot_first);
        }
    }

    #[test]
    fn gauntlet_is_stable_for_a_date() {
        let seed = gauntlet_seed("2026-07-03");
        let a = gauntlet(seed);
        let b = gauntlet(seed);
        assert_eq!(a.len(), GAUNTLET_SIZE);
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.f, y.f);
        }
        // Different date, different formulas (with overwhelming probability).
        let other = gauntlet(gauntlet_seed("2026-07-04"));
        assert!(a.iter().zip(&other).any(|(x, y)| x.f != y.f));
    }

    #[test]
    fn share_codes_round_trip() {
        for seed in [0u32, 1, 0xDEAD_BEEF, u32::MAX, 0x7099_1e5a] {
            let code = share_code(seed);
            assert_eq!(parse_share_code(&code), Some(seed), "{code}");
        }
        assert_eq!(parse_share_code("TPL-000000O"), Some(0)); // Crockford O→0
        assert_eq!(parse_share_code("garbage"), None);
    }
}
