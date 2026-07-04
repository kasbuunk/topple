//! Online duel wire format. A match is a tiny replayable event log: the
//! header fixes the deal (both devices generate the identical formula from
//! the seed), then every choice — the pie rule and each assignment — is one
//! event. The full blob travels as Game Center match data, so any device can
//! rebuild the whole match from scratch at any time.
//!
//! Roles: P1 is the match creator. P1 prices the formula and picks a side;
//! P2 picks who assigns first. Sides then alternate assignments.

use topple_core::{apply_move, deal, winner, Deal, DealKind, GenParams, Move, Rng, Side, F};

pub const MAGIC: &[u8; 4] = b"TPLM";
const VERSION: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// P1 takes this side (P2 gets the other).
    PickSide(Side),
    /// P2 decides which side assigns first.
    PickOrder(Side),
    /// The side to move assigns an atom.
    Assign(Move),
}

/// Whose input the match is waiting on, or the winning side if it is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Actor {
    P1,
    P2,
    Over(Side),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchState {
    pub seed: u64,
    pub level: u8,
    pub events: Vec<Event>,
}

fn side_byte(s: Side) -> u8 {
    match s {
        Side::Top => 0,
        Side::Bot => 1,
    }
}

fn byte_side(b: u8) -> Option<Side> {
    match b {
        0 => Some(Side::Top),
        1 => Some(Side::Bot),
        _ => None,
    }
}

impl MatchState {
    pub fn new(seed: u64, level: u8) -> MatchState {
        MatchState {
            seed,
            level: level.clamp(1, 5),
            events: Vec::new(),
        }
    }

    /// The priced deal. Deterministic: same seed and level, same board on
    /// both devices.
    pub fn deal_full(&self) -> Deal {
        let mut rng = Rng::new(self.seed);
        let params = GenParams::difficulty(self.level);
        deal(&mut rng, &params, DealKind::Any)
    }

    /// The dealt formula.
    pub fn formula(&self) -> F {
        self.deal_full().f
    }

    pub fn p1_side(&self) -> Option<Side> {
        self.events.iter().find_map(|e| match e {
            Event::PickSide(s) => Some(*s),
            _ => None,
        })
    }

    pub fn first(&self) -> Option<Side> {
        self.events.iter().find_map(|e| match e {
            Event::PickOrder(s) => Some(*s),
            _ => None,
        })
    }

    pub fn assigns(&self) -> impl Iterator<Item = Move> + '_ {
        self.events.iter().filter_map(|e| match e {
            Event::Assign(mv) => Some(*mv),
            _ => None,
        })
    }

    /// Replay every assignment onto the dealt board. `None` if an event is
    /// illegal (a corrupt or malicious blob).
    pub fn board(&self) -> Option<(F, Option<Side>)> {
        let mut f = self.formula();
        for mv in self.assigns() {
            if winner(&f).is_some() {
                return None; // moves after the game ended
            }
            let (next, _) = apply_move(&f, mv).ok()?;
            f = next;
        }
        let w = winner(&f);
        Some((f, w))
    }

    /// The side to move after the recorded assignments.
    pub fn to_move(&self) -> Option<Side> {
        let first = self.first()?;
        let n = self.assigns().count();
        Some(if n.is_multiple_of(2) { first } else { first.other() })
    }

    /// Who acts next. `None` if the log is malformed.
    pub fn next_actor(&self) -> Option<Actor> {
        match self.events.len() {
            0 => Some(Actor::P1),
            1 => Some(Actor::P2),
            _ => {
                let (_, outcome) = self.board()?;
                if let Some(w) = outcome {
                    return Some(Actor::Over(w));
                }
                let p1 = self.p1_side()?;
                let side = self.to_move()?;
                Some(if side == p1 { Actor::P1 } else { Actor::P2 })
            }
        }
    }

    /// Structural sanity: pick-side, then pick-order, then only assigns —
    /// and the assigns replay legally.
    pub fn valid(&self) -> bool {
        for (i, e) in self.events.iter().enumerate() {
            let ok = match e {
                Event::PickSide(_) => i == 0,
                Event::PickOrder(_) => i == 1,
                Event::Assign(_) => i >= 2,
            };
            if !ok {
                return false;
            }
        }
        self.board().is_some()
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(16 + self.events.len() * 3);
        v.extend_from_slice(MAGIC);
        v.push(VERSION);
        v.extend_from_slice(&self.seed.to_le_bytes());
        v.push(self.level);
        v.push(self.events.len() as u8);
        for e in &self.events {
            match e {
                Event::PickSide(s) => v.extend_from_slice(&[0, side_byte(*s)]),
                Event::PickOrder(s) => v.extend_from_slice(&[1, side_byte(*s)]),
                Event::Assign(mv) => v.extend_from_slice(&[2, mv.atom, mv.value as u8]),
            }
        }
        v
    }

    pub fn decode(b: &[u8]) -> Option<MatchState> {
        if b.len() < 15 || &b[..4] != MAGIC || b[4] != VERSION {
            return None;
        }
        let seed = u64::from_le_bytes(b[5..13].try_into().ok()?);
        let level = b[13];
        if !(1..=5).contains(&level) {
            return None;
        }
        let n = b[14] as usize;
        let mut events = Vec::with_capacity(n);
        let mut i = 15;
        for _ in 0..n {
            let tag = *b.get(i)?;
            match tag {
                0 => {
                    events.push(Event::PickSide(byte_side(*b.get(i + 1)?)?));
                    i += 2;
                }
                1 => {
                    events.push(Event::PickOrder(byte_side(*b.get(i + 1)?)?));
                    i += 2;
                }
                2 => {
                    let atom = *b.get(i + 1)?;
                    let value = match *b.get(i + 2)? {
                        0 => false,
                        1 => true,
                        _ => return None,
                    };
                    events.push(Event::Assign(Move::new(atom, value)));
                    i += 3;
                }
                _ => return None,
            }
        }
        if i != b.len() {
            return None;
        }
        let st = MatchState {
            seed,
            level,
            events,
        };
        st.valid().then_some(st)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_round_trips() {
        let mut st = MatchState::new(0xDEAD_BEEF_0BAD_F00D, 3);
        st.events.push(Event::PickSide(Side::Top));
        st.events.push(Event::PickOrder(Side::Bot));
        // A legal first assignment: pick a real atom off the dealt board.
        let atom = st.formula().atoms()[0];
        st.events.push(Event::Assign(Move::new(atom, true)));
        let blob = st.encode();
        assert_eq!(MatchState::decode(&blob), Some(st));
    }

    #[test]
    fn garbage_is_rejected() {
        assert_eq!(MatchState::decode(b""), None);
        assert_eq!(MatchState::decode(b"TPLMxxxxxxxxxxxxxxxx"), None);
        // Assign before the pie rule is structurally invalid.
        let mut st = MatchState::new(7, 1);
        st.events.push(Event::Assign(Move::new(0, true)));
        assert_eq!(MatchState::decode(&st.encode()), None);
    }

    #[test]
    fn same_seed_same_formula() {
        let a = MatchState::new(42, 4).formula();
        let b = MatchState::new(42, 4).formula();
        assert_eq!(topple_core::pretty(&a), topple_core::pretty(&b));
    }

    #[test]
    fn actor_progression() {
        let mut st = MatchState::new(99, 2);
        assert_eq!(st.next_actor(), Some(Actor::P1));
        st.events.push(Event::PickSide(Side::Bot));
        assert_eq!(st.next_actor(), Some(Actor::P2));
        st.events.push(Event::PickOrder(Side::Bot));
        // ⊥ assigns first and P1 is ⊥, so P1 acts.
        assert_eq!(st.next_actor(), Some(Actor::P1));
        assert_eq!(st.to_move(), Some(Side::Bot));
    }
}
