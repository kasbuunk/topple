//! The whole game outside a single round: title, modes, pie-rule setup,
//! pause, proof review. One state machine, one framebuffer.

use crate::fb::{Frame, HEIGHT, WIDTH};
use crate::font::FontEngine;
use crate::input::{menu_taps, Button, Repeater, TapAction, TapZone};
use crate::online::{Actor, Event, MatchState};
use crate::round::{PlayerKind, Round, RoundCfg};
use crate::save::SaveData;
use crate::theme;
use topple_core::{
    atom_name, builtin_puzzles, deal, gauntlet, gauntlet_seed, parse_share_code, pretty,
    share_code, Deal, DealKind, GenParams, Move, Puzzle, Rng, Side, Solver,
};

const RULES_CARD: &[&str] = &[
    "⊤OPP⊥E ─────────────────────────────",
    "One formula. ⊤ wants it to end ⊤;",
    "⊥ wants it to end ⊥.",
    "",
    "SETUP  Formula appears. One player",
    "picks a side; the other picks who",
    "moves first.",
    "",
    "TURN   Pick any atom on the board and",
    "set it to ⊤ or ⊥. Every occurrence is",
    "replaced at once. You must move.",
    "",
    "LAWS   The board then rewrites itself,",
    "one law at a time (mirrors apply too):",
    " ⊤∧x=x   ⊥∧x=⊥   ⊤∨x=⊤   ⊥∨x=x",
    " ⊤⇒x=x   ⊥⇒x=⊤   x⇒⊤=⊤   x⇒⊥=¬x",
    " (⊤=x)=x (⊥=x)=¬x ¬⊤=⊥ ¬⊥=⊤ ¬¬x=x",
    "",
    "WIN    The instant a lone ⊤ or ⊥",
    "remains, that side wins. No draws.",
];

const B32: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

#[derive(Clone, Copy, PartialEq, Eq)]
enum After {
    Play,
    PickOrder,
    Title,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TitleItem {
    Duel,
    Online,
    Adversary,
    Puzzles,
    Gauntlet,
    Rules,
    Strict,
    Quit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DiffTarget {
    Duel,
    Adversary,
    Online,
}

/// What the platform shim should do about an online match right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnlineStatus {
    /// No online match is active in the app.
    Inactive,
    /// Waiting on the local player's input.
    LocalTurn,
    /// Waiting on the opponent's move.
    RemoteTurn,
    /// Match over: the local player won.
    WonLocal,
    /// Match over: the opponent won.
    WonRemote,
}

enum Screen {
    Title {
        sel: usize,
    },
    Rules {
        from_pause: bool,
    },
    Difficulty {
        sel: usize,
        target: DiffTarget,
    },
    PuzzleList {
        sel: usize,
    },
    GauntletMenu {
        sel: usize,
    },
    CodeEntry {
        chars: [usize; 7],
        pos: usize,
    },
    PickSide,
    PickOrder {
        sel: usize,
    },
    Notice {
        title: String,
        lines: Vec<String>,
        next: After,
    },
    Play,
    Pause {
        sel: usize,
    },
    RoundOver {
        sel: usize,
        title: String,
        win: bool,
    },
    Proof {
        scroll: i32,
    },
    GauntletSummary,
    /// Online duel: nothing to do but wait for the opponent's choice.
    OnlineWait {
        title: String,
        lines: Vec<String>,
    },
}

enum Mode {
    Duel {
        level: u8,
        round_no: u32,
        score: [u32; 2],
    },
    Adversary {
        level: u8,
        round_no: u32,
        you: u32,
        adv: u32,
    },
    Puzzle {
        index: usize,
    },
    Gauntlet {
        seed: u32,
        idx: usize,
        results: Vec<bool>,
        deals: Vec<Deal>,
    },
    Online(OnlineState),
}

/// A live online duel: the canonical event log plus how much of it the
/// screens and round already reflect.
struct OnlineState {
    st: MatchState,
    local_p1: bool,
    /// Events already applied to the current screen/round.
    applied: usize,
    /// `encode()` has bytes the platform shim has not shipped yet.
    dirty: bool,
}

/// Pie-rule state between the deal and the first assignment.
struct Pending {
    deal: Deal,
    /// Duel: the player who picked the side. Vs-AI: always the human.
    picked_side: Option<Side>,
    /// Vs-AI even rounds: the AI's side (human picks order).
    ai_side: Option<Side>,
}

pub struct App {
    fonts: FontEngine,
    rng: Rng,
    date_iso: String,
    screen: Screen,
    mode: Option<Mode>,
    pending: Option<Pending>,
    round: Option<Round>,
    repeater: Repeater,
    pub save: SaveData,
    save_dirty: bool,
    wants_exit: bool,
    over_timer: u32,
    puzzles: Vec<Puzzle>,
    /// Platform offers online matches (shows "online duel" on the title).
    online_available: bool,
    /// Platform hides "quit" (iOS apps must not exit themselves).
    can_quit: bool,
    /// The player asked for an online match at this difficulty.
    online_request: Option<u8>,
    /// The player resigned the online match from the pause menu.
    online_resign: bool,
    /// Tappable regions, rebuilt on every render.
    taps: Vec<TapZone>,
}

impl App {
    pub fn new(seed: u64, date_iso: &str) -> App {
        App {
            fonts: FontEngine::new(),
            rng: Rng::new(seed),
            date_iso: date_iso.to_string(),
            screen: Screen::Title { sel: 0 },
            mode: None,
            pending: None,
            round: None,
            repeater: Repeater::new(),
            save: SaveData::default(),
            save_dirty: false,
            wants_exit: false,
            over_timer: 0,
            puzzles: builtin_puzzles(),
            online_available: false,
            can_quit: true,
            online_request: None,
            online_resign: false,
            taps: Vec::new(),
        }
    }

    /// Platform capabilities: whether online duels can be arranged, and
    /// whether the app may offer to quit (not on iOS).
    pub fn configure_platform(&mut self, online: bool, can_quit: bool) {
        self.online_available = online;
        self.can_quit = can_quit;
    }

    pub fn wants_exit(&self) -> bool {
        self.wants_exit
    }

    /// Which screen is showing — for tests and the headless harness.
    pub fn screen_name(&self) -> &'static str {
        match self.screen {
            Screen::Title { .. } => "title",
            Screen::Rules { .. } => "rules",
            Screen::Difficulty { .. } => "difficulty",
            Screen::PuzzleList { .. } => "puzzle-list",
            Screen::GauntletMenu { .. } => "gauntlet-menu",
            Screen::CodeEntry { .. } => "code-entry",
            Screen::PickSide => "pick-side",
            Screen::PickOrder { .. } => "pick-order",
            Screen::Notice { .. } => "notice",
            Screen::Play => "play",
            Screen::Pause { .. } => "pause",
            Screen::RoundOver { .. } => "round-over",
            Screen::Proof { .. } => "proof",
            Screen::GauntletSummary => "gauntlet-summary",
            Screen::OnlineWait { .. } => "online-wait",
        }
    }

    pub fn load_save(&mut self, blob: Option<&[u8]>) {
        if let Some(s) = blob.and_then(SaveData::from_bytes) {
            self.save = s;
        }
    }

    /// Returns a save blob when something changed since the last call.
    pub fn take_save(&mut self) -> Option<Vec<u8>> {
        if self.save_dirty {
            self.save_dirty = false;
            Some(self.save.to_bytes())
        } else {
            None
        }
    }

    // ------------------------------------------------------------- online --

    /// The player asked the platform to arrange an online duel at this
    /// difficulty. One-shot.
    pub fn take_online_request(&mut self) -> Option<u8> {
        self.online_request.take()
    }

    /// The player resigned the online match from the pause menu. One-shot.
    pub fn take_online_resign(&mut self) -> bool {
        std::mem::take(&mut self.online_resign)
    }

    /// Start a brand-new online match as its creator (P1). The header goes
    /// straight to the outbox so the opponent's device can deal the same
    /// formula.
    pub fn online_create(&mut self, seed: u64, level: u8, local_p1: bool) {
        self.install_online(MatchState::new(seed, level), local_p1);
        if let Some(Mode::Online(o)) = &mut self.mode {
            o.dirty = true;
        }
    }

    /// Feed the latest match blob from the wire. Extending the live match
    /// applies just the new events (animating a fresh remote move); anything
    /// else rebuilds from scratch. Returns false on a corrupt blob.
    pub fn online_load(&mut self, data: &[u8], local_p1: bool) -> bool {
        let Some(st) = MatchState::decode(data) else {
            return false;
        };
        if let Some(Mode::Online(o)) = &mut self.mode {
            if o.st.seed == st.seed && o.st.level == st.level && o.local_p1 == local_p1 {
                let ours = o.st.events.len();
                if st.events.len() >= ours && st.events[..ours] == o.st.events[..] {
                    o.st = st;
                    self.online_refresh(true);
                    return true;
                }
                if st.events.len() < ours && o.st.events[..st.events.len()] == st.events[..] {
                    // A stale echo of data we already have.
                    return true;
                }
            }
        }
        self.install_online(st, local_p1);
        true
    }

    fn install_online(&mut self, st: MatchState, local_p1: bool) {
        self.mode = Some(Mode::Online(OnlineState {
            st,
            local_p1,
            applied: 0,
            dirty: false,
        }));
        self.pending = None;
        self.round = None;
        self.online_refresh(false);
    }

    /// Fresh match blob to ship, if any. One-shot.
    pub fn take_online_outbox(&mut self) -> Option<Vec<u8>> {
        if let Some(Mode::Online(o)) = &mut self.mode {
            if o.dirty {
                o.dirty = false;
                return Some(o.st.encode());
            }
        }
        None
    }

    /// Whose input the live online match is waiting on.
    pub fn online_status(&self) -> OnlineStatus {
        let Some(Mode::Online(o)) = &self.mode else {
            return OnlineStatus::Inactive;
        };
        match o.st.next_actor() {
            None => OnlineStatus::Inactive,
            Some(Actor::P1) if o.local_p1 => OnlineStatus::LocalTurn,
            Some(Actor::P1) => OnlineStatus::RemoteTurn,
            Some(Actor::P2) if o.local_p1 => OnlineStatus::RemoteTurn,
            Some(Actor::P2) => OnlineStatus::LocalTurn,
            Some(Actor::Over(w)) => {
                let Some(p1s) = o.st.p1_side() else {
                    return OnlineStatus::Inactive;
                };
                let local = if o.local_p1 { p1s } else { p1s.other() };
                if w == local {
                    OnlineStatus::WonLocal
                } else {
                    OnlineStatus::WonRemote
                }
            }
        }
    }

    /// The opponent quit or the platform ended the match: tell the player.
    pub fn online_opponent_quit(&mut self) {
        if !matches!(self.mode, Some(Mode::Online(_))) {
            return;
        }
        self.mode = None;
        self.round = None;
        self.pending = None;
        self.screen = Screen::Notice {
            title: "online duel".into(),
            lines: vec![
                "The opponent has left the match.".into(),
                "The win is yours.".into(),
            ],
            next: After::Title,
        };
    }

    /// A local pie-rule choice goes on the wire, then the screens catch up.
    fn online_push(&mut self, e: Event) {
        if let Some(Mode::Online(o)) = &mut self.mode {
            o.st.events.push(e);
            o.dirty = true;
        }
        self.online_refresh(false);
    }

    /// A local assignment just landed in the round's history: log it.
    fn online_record_local(&mut self, hist_before: usize) {
        let Some(r) = &self.round else { return };
        if r.history.len() <= hist_before {
            return;
        }
        let rec = r.history.last().expect("history just grew");
        if !r.player(rec.mover).is_human() {
            return;
        }
        let mv = rec.mv;
        if let Some(Mode::Online(o)) = &mut self.mode {
            o.st.events.push(Event::Assign(mv));
            o.applied += 1;
            o.dirty = true;
        }
    }

    /// Make the screens and round reflect the event log. `animate` plays the
    /// newest remote assignment with the full cascade.
    fn online_refresh(&mut self, animate: bool) {
        let Some(Mode::Online(o)) = &mut self.mode else {
            return;
        };
        let n = o.st.events.len();
        // The two pie-rule events carry no round state; mark them applied.
        o.applied = o.applied.max(n.min(2));
        let (st, local_p1, applied) = (o.st.clone(), o.local_p1, o.applied);
        let Some(actor) = st.next_actor() else {
            self.go_title();
            return;
        };
        if n < 2 {
            // Pie-rule phase: P1 picks a side, P2 picks the tempo.
            self.pending = Some(Pending {
                deal: st.deal_full(),
                picked_side: st.p1_side(),
                ai_side: None,
            });
            let local_acts = matches!((actor, local_p1), (Actor::P1, true) | (Actor::P2, false));
            self.screen = if local_acts {
                if n == 0 {
                    Screen::PickSide
                } else {
                    Screen::PickOrder { sel: 0 }
                }
            } else {
                let lines = if n == 0 {
                    vec!["The opponent is pricing the formula…".into()]
                } else {
                    let yours = st.p1_side().expect("one event means a side was picked");
                    vec![
                        format!("You take {}.", yours.glyph()),
                        "The opponent chooses who assigns first…".into(),
                    ]
                };
                Screen::OnlineWait {
                    title: "online duel · the pie rule".into(),
                    lines,
                }
            };
            return;
        }
        // Play phase.
        let p1s = st.p1_side().expect("valid log has a side");
        let first = st.first().expect("valid log has an order");
        let local_side = if local_p1 { p1s } else { p1s.other() };
        if self.round.is_none() {
            let (top, bot) = if local_side == Side::Top {
                (PlayerKind::Human(1), PlayerKind::Remote)
            } else {
                (PlayerKind::Remote, PlayerKind::Human(1))
            };
            self.round = Some(Round::new(
                st.deal_full().f,
                first,
                top,
                bot,
                RoundCfg {
                    allow_preview: !self.strict(),
                    label: "ONLINE DUEL".into(),
                    status: format!("you are {}", local_side.glyph()),
                },
            ));
            self.over_timer = 0;
        }
        let assigns: Vec<Move> = st.assigns().collect();
        // Events 0 and 1 are the pie rule, so assign k is event k + 2.
        if let Some(r) = &mut self.round {
            for (k, mv) in assigns.iter().enumerate().skip(applied.saturating_sub(2)) {
                if animate && k + 2 == n && r.player(r.to_move) == PlayerKind::Remote {
                    r.apply_remote_move(*mv);
                } else {
                    r.apply_move_instant(*mv);
                }
            }
        }
        if let Some(Mode::Online(o)) = &mut self.mode {
            o.applied = n;
        }
        if !matches!(
            self.screen,
            Screen::Pause { .. } | Screen::Proof { .. } | Screen::RoundOver { .. }
        ) {
            self.screen = Screen::Play;
        }
    }

    // ------------------------------------------------------------- events --

    pub fn on_press(&mut self, b: Button) {
        self.repeater.press(b);
        self.handle(b);
    }

    pub fn on_release(&mut self, b: Button) {
        self.repeater.release(b);
    }

    /// A tap at framebuffer coordinates. Zones come from the latest render,
    /// so call this only after at least one `render`.
    pub fn on_tap(&mut self, x: f32, y: f32) {
        // Later zones sit on top (overlays register after the board).
        let act = self
            .taps
            .iter()
            .rev()
            .find(|z| z.hit(x, y))
            .map(|z| z.act.clone());
        match act {
            Some(TapAction::Press(seq)) => {
                for b in seq {
                    self.handle(b);
                }
            }
            Some(TapAction::Cursor(oi)) => {
                if let Some(r) = &mut self.round {
                    r.set_cursor(oi);
                }
            }
            None => {}
        }
    }

    /// The current tappable regions — for tests and platform shims.
    pub fn tap_zones(&self) -> &[TapZone] {
        &self.taps
    }

    /// The atom under the play cursor — for tests and platform shims.
    pub fn round_hovered_atom(&self) -> Option<topple_core::Atom> {
        self.round.as_ref().and_then(|r| r.hovered_atom())
    }

    pub fn tick(&mut self, dt_ms: u32) {
        for b in self.repeater.tick(dt_ms) {
            self.handle(b);
        }
        if let (Screen::Play, Some(round)) = (&self.screen, &mut self.round) {
            round.tick(dt_ms, &mut self.rng);
            if round.outcome.is_some() && !round.animating() {
                self.over_timer += dt_ms;
                if self.over_timer >= 1100 {
                    self.finish_round();
                }
            }
        }
    }

    // -------------------------------------------------------------- modes --

    fn strict(&self) -> bool {
        self.save.strict
    }

    fn deal_params(&self, level: u8) -> GenParams {
        GenParams::difficulty(level)
    }

    fn start_duel(&mut self, level: u8) {
        self.mode = Some(Mode::Duel {
            level,
            round_no: 1,
            score: [0, 0],
        });
        self.next_duel_round();
    }

    fn next_duel_round(&mut self) {
        let Some(Mode::Duel { level, .. }) = &self.mode else {
            return;
        };
        let params = self.deal_params(*level);
        let d = deal(&mut self.rng, &params, DealKind::Any);
        self.pending = Some(Pending {
            deal: d,
            picked_side: None,
            ai_side: None,
        });
        self.screen = Screen::PickSide;
    }

    fn start_adversary(&mut self, level: u8) {
        self.save.level = level;
        self.save_dirty = true;
        self.mode = Some(Mode::Adversary {
            level,
            round_no: 1,
            you: 0,
            adv: 0,
        });
        self.next_adversary_round();
    }

    fn next_adversary_round(&mut self) {
        let Some(Mode::Adversary {
            level, round_no, ..
        }) = &self.mode
        else {
            return;
        };
        let (level, round_no) = (*level, *round_no);
        if round_no % 2 == 1 {
            // You price the formula and pick a side; the Adversary replies
            // with the tempo. Correct judgment + correct play wins.
            let params = self.deal_params(level);
            let d = deal(&mut self.rng, &params, DealKind::SideDominant);
            self.pending = Some(Pending {
                deal: d,
                picked_side: None,
                ai_side: None,
            });
            self.screen = Screen::PickSide;
        } else {
            // The Adversary takes a side; you pick who assigns first.
            let params = self.deal_params(level);
            let d = deal(&mut self.rng, &params, DealKind::TempoSensitive);
            let ai = if self.rng.below(2) == 0 {
                Side::Top
            } else {
                Side::Bot
            };
            self.pending = Some(Pending {
                deal: d,
                picked_side: None,
                ai_side: Some(ai),
            });
            self.screen = Screen::Notice {
                title: "the pie rule".into(),
                lines: vec![
                    format!("The Adversary takes {}.", ai.glyph()),
                    format!("You hold {}. Choose the tempo.", ai.other().glyph()),
                ],
                next: After::PickOrder,
            };
        }
    }

    fn start_puzzle(&mut self, index: usize) {
        let pz = &self.puzzles[index];
        self.mode = Some(Mode::Puzzle { index });
        let you = pz.you;
        let (top, bot) = if you == Side::Top {
            (PlayerKind::Human(1), PlayerKind::Adversary)
        } else {
            (PlayerKind::Adversary, PlayerKind::Human(1))
        };
        self.round = Some(Round::new(
            pz.f.clone(),
            you,
            top,
            bot,
            RoundCfg {
                allow_preview: false,
                label: format!("PUZZLE · {}", pz.title),
                status: format!("you are {} · move first", you.glyph()),
            },
        ));
        self.over_timer = 0;
        self.screen = Screen::Play;
    }

    fn start_gauntlet(&mut self, seed: u32) {
        let deals = gauntlet(seed);
        self.mode = Some(Mode::Gauntlet {
            seed,
            idx: 0,
            results: Vec::new(),
            deals,
        });
        self.next_gauntlet_round();
    }

    fn next_gauntlet_round(&mut self) {
        let Some(Mode::Gauntlet {
            seed, idx, deals, ..
        }) = &self.mode
        else {
            return;
        };
        let (seed, idx) = (*seed, *idx);
        if idx >= deals.len() {
            self.screen = Screen::GauntletSummary;
            return;
        }
        let d = deals[idx].clone();
        if d.side_dominant().is_some() {
            self.pending = Some(Pending {
                deal: d,
                picked_side: None,
                ai_side: None,
            });
            self.screen = Screen::PickSide;
        } else {
            // Deterministic AI side so a shared code is the same challenge.
            let ai = if Rng::new(seed as u64 * 1009 + idx as u64).below(2) == 0 {
                Side::Top
            } else {
                Side::Bot
            };
            self.pending = Some(Pending {
                deal: d,
                picked_side: None,
                ai_side: Some(ai),
            });
            self.screen = Screen::Notice {
                title: format!("gauntlet {} of 5", idx + 1),
                lines: vec![
                    format!("The Adversary takes {}.", ai.glyph()),
                    format!("You hold {}. Choose the tempo.", ai.other().glyph()),
                ],
                next: After::PickOrder,
            };
        }
    }

    fn title_items(&self) -> Vec<TitleItem> {
        let mut v = vec![TitleItem::Duel];
        if self.online_available {
            v.push(TitleItem::Online);
        }
        v.extend([
            TitleItem::Adversary,
            TitleItem::Puzzles,
            TitleItem::Gauntlet,
            TitleItem::Rules,
            TitleItem::Strict,
        ]);
        if self.can_quit {
            v.push(TitleItem::Quit);
        }
        v
    }

    fn title_label(&self, it: TitleItem) -> String {
        match it {
            TitleItem::Duel => "duel".into(),
            TitleItem::Online => "online duel".into(),
            TitleItem::Adversary => "adversary".into(),
            TitleItem::Puzzles => "puzzles".into(),
            TitleItem::Gauntlet => "daily gauntlet".into(),
            TitleItem::Rules => "rules".into(),
            TitleItem::Strict => {
                if self.save.strict {
                    "strict mode · on".into()
                } else {
                    "strict mode · off".into()
                }
            }
            TitleItem::Quit => "quit".into(),
        }
    }

    fn title_index(&self, it: TitleItem) -> usize {
        self.title_items()
            .iter()
            .position(|&x| x == it)
            .unwrap_or(0)
    }

    fn vs_ai(&self) -> bool {
        matches!(
            self.mode,
            Some(Mode::Adversary { .. }) | Some(Mode::Gauntlet { .. }) | Some(Mode::Puzzle { .. })
        )
    }

    fn mode_label(&self) -> String {
        match &self.mode {
            Some(Mode::Duel { round_no, .. }) => format!("DUEL · ROUND {round_no}"),
            Some(Mode::Adversary {
                level, round_no, ..
            }) => {
                format!("ADVERSARY · LV{level} · ROUND {round_no}")
            }
            Some(Mode::Puzzle { index }) => format!("PUZZLE · {}", self.puzzles[*index].title),
            Some(Mode::Gauntlet { idx, .. }) => format!("GAUNTLET · {} OF 5", idx + 1),
            Some(Mode::Online(_)) => "ONLINE DUEL".to_string(),
            None => String::new(),
        }
    }

    fn mode_status(&self) -> String {
        match &self.mode {
            Some(Mode::Duel { score, .. }) => format!("P1 {} — {} P2", score[0], score[1]),
            Some(Mode::Adversary { you, adv, .. }) => format!("you {you} — {adv} adv · to 3"),
            Some(Mode::Puzzle { index }) => {
                let pz = &self.puzzles[*index];
                format!("{} in {}", pz.you.glyph(), pz.mate_in)
            }
            Some(Mode::Gauntlet { results, .. }) => {
                format!("score {}", results.iter().filter(|&&r| r).count())
            }
            Some(Mode::Online(o)) => match (o.st.p1_side(), o.local_p1) {
                (Some(p1s), true) => format!("you are {}", p1s.glyph()),
                (Some(p1s), false) => format!("you are {}", p1s.other().glyph()),
                (None, _) => String::new(),
            },
            None => String::new(),
        }
    }

    /// Perfect pie-rule reply: pick the first-mover that wins for `ai_side`,
    /// or failing that the order that drags longest.
    fn ai_pick_order(&self, f: &topple_core::F, ai_side: Side) -> Side {
        let mut s = Solver::new();
        let top_first = s.solve(f, Side::Top);
        let bot_first = s.solve(f, Side::Bot);
        match (top_first.winner == ai_side, bot_first.winner == ai_side) {
            (true, _) => Side::Top,
            (_, true) => Side::Bot,
            _ => {
                if top_first.plies >= bot_first.plies {
                    Side::Top
                } else {
                    Side::Bot
                }
            }
        }
    }

    fn build_round(
        &mut self,
        human_or_p1_side: Side,
        first: Side,
        duel_sides: Option<(Side, Side)>,
    ) {
        let Some(p) = self.pending.take() else { return };
        let (top, bot) = if let Some((p1s, _)) = duel_sides {
            if p1s == Side::Top {
                (PlayerKind::Human(1), PlayerKind::Human(2))
            } else {
                (PlayerKind::Human(2), PlayerKind::Human(1))
            }
        } else if human_or_p1_side == Side::Top {
            (PlayerKind::Human(1), PlayerKind::Adversary)
        } else {
            (PlayerKind::Adversary, PlayerKind::Human(1))
        };
        self.round = Some(Round::new(
            p.deal.f.clone(),
            first,
            top,
            bot,
            RoundCfg {
                allow_preview: !self.strict() && !matches!(self.mode, Some(Mode::Puzzle { .. })),
                label: self.mode_label(),
                status: self.mode_status(),
            },
        ));
        self.over_timer = 0;
        self.screen = Screen::Play;
    }

    /// Round finished (board collapsed): update scores, open the end menu.
    fn finish_round(&mut self) {
        let Some(round) = &self.round else { return };
        let w = round.outcome.expect("finish_round without outcome");
        let mut title = format!("{} wins", w.glyph());
        let mut win_for_menu = true;
        match &mut self.mode {
            Some(Mode::Duel { score, .. }) => {
                let p_top = round.top_player;
                let winner_kind = if w == Side::Top {
                    p_top
                } else {
                    round.bot_player
                };
                if let PlayerKind::Human(n) = winner_kind {
                    score[(n - 1) as usize] += 1;
                    title = format!("{} wins — Player {}", w.glyph(), n);
                }
            }
            Some(Mode::Adversary { you, adv, .. }) => {
                let human_won = round.player(w).is_human();
                if human_won {
                    *you += 1;
                    title = format!("{} wins — you", w.glyph());
                } else {
                    *adv += 1;
                    title = format!("{} wins — the Adversary", w.glyph());
                }
                win_for_menu = human_won;
            }
            Some(Mode::Puzzle { index }) => {
                let human_won = round.player(w).is_human();
                if human_won {
                    let moves = round
                        .history
                        .iter()
                        .filter(|r| round.player(r.mover).is_human())
                        .count();
                    title = format!("solved in {moves}");
                    self.save.solved |= 1 << *index;
                    self.save_dirty = true;
                } else {
                    title = "refuted".into();
                }
                win_for_menu = human_won;
            }
            Some(Mode::Gauntlet { results, .. }) => {
                let human_won = round.player(w).is_human();
                results.push(human_won);
                title = if human_won {
                    format!("{} wins — you", w.glyph())
                } else {
                    format!("{} wins — the Adversary", w.glyph())
                };
                win_for_menu = human_won;
            }
            Some(Mode::Online(_)) => {
                let human_won = round.player(w).is_human();
                title = if human_won {
                    format!("{} wins — you", w.glyph())
                } else {
                    format!("{} wins — the opponent", w.glyph())
                };
                win_for_menu = human_won;
            }
            None => {}
        }
        self.screen = Screen::RoundOver {
            sel: 0,
            title,
            win: win_for_menu,
        };
    }

    fn round_over_items(&self) -> Vec<String> {
        match &self.mode {
            Some(Mode::Duel { .. }) => {
                vec![
                    "rematch".into(),
                    "review the proof".into(),
                    "main menu".into(),
                ]
            }
            Some(Mode::Adversary { you, adv, .. }) => {
                if *you >= 3 || *adv >= 3 {
                    vec![
                        "new match".into(),
                        "review the proof".into(),
                        "main menu".into(),
                    ]
                } else {
                    vec![
                        "next round".into(),
                        "review the proof".into(),
                        "main menu".into(),
                    ]
                }
            }
            Some(Mode::Puzzle { index }) => {
                let solved = self.save.solved & (1 << *index) != 0;
                let first = if solved && *index + 1 < self.puzzles.len() {
                    "next puzzle"
                } else {
                    "try again"
                };
                vec![
                    first.into(),
                    "review the proof".into(),
                    "puzzle list".into(),
                ]
            }
            Some(Mode::Gauntlet { idx, deals, .. }) => {
                let first = if idx + 1 >= deals.len() {
                    "finish"
                } else {
                    "next formula"
                };
                vec![first.into(), "review the proof".into(), "abandon".into()]
            }
            Some(Mode::Online(_)) => {
                vec!["main menu".into(), "review the proof".into()]
            }
            None => vec!["main menu".into()],
        }
    }

    fn round_over_advance(&mut self) {
        match &mut self.mode {
            Some(Mode::Duel { round_no, .. }) => {
                *round_no += 1;
                self.next_duel_round();
            }
            Some(Mode::Adversary {
                you,
                adv,
                round_no,
                level,
            }) => {
                if *you >= 3 || *adv >= 3 {
                    let lv = *level;
                    self.start_adversary(lv);
                } else {
                    *round_no += 1;
                    self.next_adversary_round();
                }
            }
            Some(Mode::Puzzle { index }) => {
                let i = *index;
                if self.save.solved & (1 << i) != 0 && i + 1 < self.puzzles.len() {
                    self.start_puzzle(i + 1);
                } else {
                    self.start_puzzle(i);
                }
            }
            Some(Mode::Gauntlet { idx, deals, .. }) => {
                *idx += 1;
                if *idx >= deals.len() {
                    self.screen = Screen::GauntletSummary;
                } else {
                    self.next_gauntlet_round();
                }
            }
            Some(Mode::Online(_)) => self.go_title(),
            None => self.go_title(),
        }
    }

    fn go_title(&mut self) {
        self.mode = None;
        self.round = None;
        self.pending = None;
        self.screen = Screen::Title { sel: 0 };
    }

    // ------------------------------------------------------------ handling --

    fn handle_title(&mut self, b: Button, sel: usize) {
        let items = self.title_items();
        let n = items.len();
        match b {
            Button::Up => {
                self.screen = Screen::Title {
                    sel: (sel + n - 1) % n,
                }
            }
            Button::Down => self.screen = Screen::Title { sel: (sel + 1) % n },
            Button::A | Button::Start => match items[sel.min(n - 1)] {
                TitleItem::Duel => {
                    self.screen = Screen::Difficulty {
                        sel: 1,
                        target: DiffTarget::Duel,
                    }
                }
                TitleItem::Online => {
                    self.screen = Screen::Difficulty {
                        sel: 1,
                        target: DiffTarget::Online,
                    }
                }
                TitleItem::Adversary => {
                    let lv = self.save.level;
                    self.screen = Screen::Difficulty {
                        sel: (lv - 1) as usize,
                        target: DiffTarget::Adversary,
                    }
                }
                TitleItem::Puzzles => self.screen = Screen::PuzzleList { sel: 0 },
                TitleItem::Gauntlet => self.screen = Screen::GauntletMenu { sel: 0 },
                TitleItem::Rules => self.screen = Screen::Rules { from_pause: false },
                TitleItem::Strict => {
                    self.save.strict = !self.save.strict;
                    self.save_dirty = true;
                }
                TitleItem::Quit => self.wants_exit = true,
            },
            _ => {}
        }
    }

    fn handle(&mut self, b: Button) {
        if let Screen::Title { sel } = &self.screen {
            let sel = *sel;
            self.handle_title(b, sel);
            return;
        }
        // Where "back" lands on the title menu (it shifts with platform items).
        let t_duel = self.title_index(TitleItem::Duel);
        let t_online = self.title_index(TitleItem::Online);
        let t_adv = self.title_index(TitleItem::Adversary);
        let t_puzzles = self.title_index(TitleItem::Puzzles);
        let t_gauntlet = self.title_index(TitleItem::Gauntlet);
        let t_rules = self.title_index(TitleItem::Rules);
        match &mut self.screen {
            Screen::Title { .. } => {}
            Screen::Rules { from_pause } => {
                if matches!(b, Button::A | Button::B | Button::Start) {
                    self.screen = if *from_pause {
                        Screen::Pause { sel: 0 }
                    } else {
                        Screen::Title { sel: t_rules }
                    };
                }
            }
            Screen::Difficulty { sel, target } => match b {
                Button::Up => *sel = (*sel + 4) % 5,
                Button::Down => *sel = (*sel + 1) % 5,
                Button::B => {
                    self.screen = Screen::Title {
                        sel: match *target {
                            DiffTarget::Duel => t_duel,
                            DiffTarget::Online => t_online,
                            DiffTarget::Adversary => t_adv,
                        },
                    }
                }
                Button::A | Button::Start => {
                    let (lv, target) = (*sel as u8 + 1, *target);
                    match target {
                        DiffTarget::Duel => self.start_duel(lv),
                        DiffTarget::Adversary => self.start_adversary(lv),
                        DiffTarget::Online => {
                            // The platform shim picks this up and arranges a
                            // match; the title stays underneath its UI.
                            self.online_request = Some(lv);
                            self.screen = Screen::Title { sel: t_online };
                        }
                    }
                }
                _ => {}
            },
            Screen::PuzzleList { sel } => {
                let n = self.puzzles.len();
                match b {
                    Button::Up => *sel = (*sel + n - 1) % n,
                    Button::Down => *sel = (*sel + 1) % n,
                    Button::B => self.screen = Screen::Title { sel: t_puzzles },
                    Button::A | Button::Start => {
                        let i = *sel;
                        self.start_puzzle(i);
                    }
                    _ => {}
                }
            }
            Screen::GauntletMenu { sel } => match b {
                Button::Up | Button::Down => *sel = 1 - *sel,
                Button::B => self.screen = Screen::Title { sel: t_gauntlet },
                Button::A | Button::Start => {
                    if *sel == 0 {
                        let seed = gauntlet_seed(&self.date_iso);
                        self.start_gauntlet(seed);
                    } else {
                        let today = share_code(gauntlet_seed(&self.date_iso));
                        let chars: Vec<usize> = today
                            .trim_start_matches("TPL-")
                            .bytes()
                            .map(|c| B32.iter().position(|&x| x == c).unwrap_or(0))
                            .collect();
                        self.screen = Screen::CodeEntry {
                            chars: chars.try_into().unwrap_or([0; 7]),
                            pos: 0,
                        };
                    }
                }
                _ => {}
            },
            Screen::CodeEntry { chars, pos } => match b {
                Button::Left => *pos = (*pos + 6) % 7,
                Button::Right => *pos = (*pos + 1) % 7,
                Button::Up => chars[*pos] = (chars[*pos] + 1) % 32,
                Button::Down => chars[*pos] = (chars[*pos] + 31) % 32,
                Button::B => self.screen = Screen::GauntletMenu { sel: 1 },
                Button::A | Button::Start => {
                    let code: String = chars.iter().map(|&c| B32[c] as char).collect();
                    if let Some(seed) = parse_share_code(&code) {
                        self.start_gauntlet(seed);
                    }
                }
                _ => {}
            },
            Screen::PickSide => {
                let side = match b {
                    Button::X => Some(Side::Top),
                    Button::B => Some(Side::Bot),
                    _ => None,
                };
                if let Some(s) = side {
                    if matches!(self.mode, Some(Mode::Online(_))) {
                        self.online_push(Event::PickSide(s));
                    } else if self.vs_ai() {
                        // The Adversary replies with a perfect tempo pick.
                        let f = self.pending.as_ref().unwrap().deal.f.clone();
                        let first = self.ai_pick_order(&f, s.other());
                        if let Some(p) = self.pending.as_mut() {
                            p.picked_side = Some(s);
                        }
                        self.screen = Screen::Notice {
                            title: "the pie rule".into(),
                            lines: vec![
                                format!("You take {}.", s.glyph()),
                                format!("The Adversary decides: {} assigns first.", first.glyph()),
                            ],
                            next: After::Play,
                        };
                        // Stash the order in ai_side (reused as "first mover").
                        if let Some(p) = self.pending.as_mut() {
                            p.ai_side = Some(first);
                        }
                    } else {
                        if let Some(p) = self.pending.as_mut() {
                            p.picked_side = Some(s);
                        }
                        self.screen = Screen::PickOrder { sel: 0 };
                    }
                }
            }
            Screen::PickOrder { sel } => match b {
                Button::Up | Button::Down => *sel = 1 - *sel,
                Button::A | Button::Start | Button::X => {
                    let sel = *sel;
                    let p = self.pending.as_ref().unwrap();
                    if matches!(self.mode, Some(Mode::Online(_))) {
                        // Online: the order picker is always P2; their side is
                        // the one P1 did not take.
                        let local = p.picked_side.expect("P1 picked before P2's turn").other();
                        let first = if sel == 0 { local } else { local.other() };
                        self.online_push(Event::PickOrder(first));
                    } else if self.vs_ai() {
                        // Options: 0 = you first, 1 = Adversary first.
                        let ai = p.ai_side.unwrap();
                        let human = ai.other();
                        let first = if sel == 0 { human } else { ai };
                        self.build_round(human, first, None);
                    } else {
                        // Duel: P(picker) chose a side; the other player chose
                        // the order. Options: 0 = P1 first, 1 = P2 first.
                        let round_no = match &self.mode {
                            Some(Mode::Duel { round_no, .. }) => *round_no,
                            _ => 1,
                        };
                        let picker_is_p1 = round_no % 2 == 1;
                        let p1_side = if picker_is_p1 {
                            p.picked_side.unwrap()
                        } else {
                            p.picked_side.unwrap().other()
                        };
                        let first = if sel == 0 { p1_side } else { p1_side.other() };
                        self.build_round(p1_side, first, Some((p1_side, p1_side.other())));
                    }
                }
                _ => {}
            },
            Screen::Notice { next, .. } => {
                if matches!(b, Button::A | Button::Start | Button::X) {
                    match next {
                        After::PickOrder => self.screen = Screen::PickOrder { sel: 0 },
                        After::Play => {
                            let p = self.pending.as_ref().unwrap();
                            let human = p.picked_side.unwrap();
                            let first = p.ai_side.unwrap();
                            self.build_round(human, first, None);
                        }
                        After::Title => self.go_title(),
                    }
                }
            }
            Screen::OnlineWait { .. } => {
                // Leaving is safe: the match lives with the matchmaking
                // service and reopens on the next turn notification.
                if matches!(b, Button::B | Button::Start) {
                    self.go_title();
                }
            }
            Screen::Play => {
                if b == Button::Start {
                    if let Some(r) = &self.round {
                        if r.outcome.is_none() {
                            self.screen = Screen::Pause { sel: 0 };
                            return;
                        }
                    }
                }
                let hist_before = self.round.as_ref().map_or(0, |r| r.history.len());
                let mut finished = false;
                if let Some(r) = &mut self.round {
                    let consumed = r.press(b, &mut self.rng, &mut self.fonts);
                    // Skip the lingering banner.
                    finished = r.outcome.is_some() && !r.animating() && !consumed;
                }
                // Online: a fresh local assignment goes on the wire.
                self.online_record_local(hist_before);
                if finished {
                    self.finish_round();
                }
            }
            Screen::Pause { sel } => {
                let items = 4;
                let online = matches!(self.mode, Some(Mode::Online(_)));
                match b {
                    Button::Up => *sel = (*sel + items - 1) % items,
                    Button::Down => *sel = (*sel + 1) % items,
                    Button::B | Button::Start => self.screen = Screen::Play,
                    Button::A => match *sel {
                        0 => self.screen = Screen::Play,
                        1 => self.screen = Screen::Rules { from_pause: true },
                        2 if online => {
                            // Leave: the match keeps waiting with the service.
                            self.go_title();
                        }
                        3 if online => {
                            self.online_resign = true;
                            self.go_title();
                        }
                        2 => {
                            // Restart the round from its opening board.
                            if let Some(r) = &self.round {
                                if let Some(first_rec) = r.history.first() {
                                    let board = first_rec.before.clone();
                                    let first =
                                        r.history.first().map(|h| h.mover).unwrap_or(r.to_move);
                                    let cfg = RoundCfg {
                                        allow_preview: r.cfg.allow_preview,
                                        label: r.cfg.label.clone(),
                                        status: r.cfg.status.clone(),
                                    };
                                    self.round = Some(Round::new(
                                        board,
                                        first,
                                        r.top_player,
                                        r.bot_player,
                                        cfg,
                                    ));
                                }
                            }
                            self.screen = Screen::Play;
                        }
                        _ => self.go_title(),
                    },
                    _ => {}
                }
            }
            Screen::RoundOver { sel, .. } => {
                let sel = *sel;
                let items = self.round_over_items().len();
                match b {
                    Button::Up => {
                        if let Screen::RoundOver { sel, .. } = &mut self.screen {
                            *sel = (*sel + items - 1) % items;
                        }
                    }
                    Button::Down => {
                        if let Screen::RoundOver { sel, .. } = &mut self.screen {
                            *sel = (*sel + 1) % items;
                        }
                    }
                    Button::A | Button::Start => match sel {
                        0 => self.round_over_advance(),
                        1 => self.screen = Screen::Proof { scroll: 0 },
                        _ => match &self.mode {
                            Some(Mode::Puzzle { .. }) => {
                                self.round = None;
                                self.screen = Screen::PuzzleList { sel: 0 };
                            }
                            _ => self.go_title(),
                        },
                    },
                    _ => {}
                }
            }
            Screen::Proof { scroll } => match b {
                Button::Up => *scroll = (*scroll - 3).max(0),
                Button::Down => *scroll += 3,
                Button::B | Button::A | Button::Start => {
                    // Back to wherever we came from.
                    if let Some(r) = &self.round {
                        let w = r.outcome;
                        if let Some(w) = w {
                            let vs_other =
                                self.vs_ai() || matches!(self.mode, Some(Mode::Online(_)));
                            self.screen = Screen::RoundOver {
                                sel: 1,
                                title: format!("{} wins", w.glyph()),
                                win: r.player(w).is_human() || !vs_other,
                            };
                            return;
                        }
                    }
                    self.go_title();
                }
                _ => {}
            },
            Screen::GauntletSummary => {
                if matches!(b, Button::A | Button::B | Button::Start) {
                    self.go_title();
                }
            }
        }
    }

    // ------------------------------------------------------------- render --

    /// Register a tappable rectangle for this frame.
    fn tap_zone(&mut self, x: f32, y: f32, w: f32, h: f32, act: TapAction) {
        self.taps.push(TapZone { x, y, w, h, act });
    }

    /// A whole-screen tap that presses one button (notices, summaries).
    fn tap_anywhere(&mut self, b: Button) {
        self.tap_zone(
            0.0,
            0.0,
            WIDTH as f32,
            HEIGHT as f32,
            TapAction::Press(vec![b]),
        );
    }

    pub fn render(&mut self, fb: &mut Frame) {
        self.taps.clear();
        match &self.screen {
            Screen::Title { .. } => self.render_title(fb),
            Screen::Rules { .. } => self.render_rules(fb),
            Screen::Difficulty { .. } => self.render_difficulty(fb),
            Screen::PuzzleList { .. } => self.render_puzzles(fb),
            Screen::GauntletMenu { .. } => self.render_gauntlet_menu(fb),
            Screen::CodeEntry { .. } => self.render_code_entry(fb),
            Screen::PickSide => self.render_pick_side(fb),
            Screen::PickOrder { .. } => self.render_pick_order(fb),
            Screen::Notice { .. } => self.render_notice(fb),
            Screen::Play => {
                if let Some(r) = &mut self.round {
                    r.render(fb, &mut self.fonts, &mut self.taps);
                }
            }
            Screen::Pause { .. } => self.render_pause(fb),
            Screen::RoundOver { .. } => self.render_round_over(fb),
            Screen::Proof { .. } => self.render_proof(fb),
            Screen::GauntletSummary => self.render_gauntlet_summary(fb),
            Screen::OnlineWait { .. } => self.render_online_wait(fb),
        }
    }

    fn draw_logo(&mut self, fb: &mut Frame, cy: f32, size: f32) {
        // The name needs no logo: ⊤OPP⊥E, the two win conditions holding up
        // the word between them.
        let text = "⊤OPP⊥E";
        let w = self.fonts.measure(size, text);
        let mut x = (WIDTH as f32 - w) / 2.0;
        for ch in text.chars() {
            let color = match ch {
                '⊤' => theme::TOP,
                '⊥' => theme::BOT,
                _ => theme::TEXT,
            };
            self.fonts.draw_char(fb, x, cy, size, color, true, ch);
            x += self.fonts.char_advance(size, ch);
        }
    }

    fn draw_menu(
        &mut self,
        fb: &mut Frame,
        items: &[(String, bool)],
        sel: usize,
        y0: f32,
        size: f32,
    ) {
        let lh = size * 1.7;
        for (i, (item, enabled)) in items.iter().enumerate() {
            let y = y0 + i as f32 * lh;
            // Tapping a row walks the selection there and confirms.
            let row_w = (self.fonts.measure(size, item) + 44.0).max(220.0);
            self.tap_zone(
                (WIDTH as f32 - row_w) / 2.0,
                y - size,
                row_w,
                lh,
                TapAction::Press(menu_taps(sel, i)),
            );
            let color = if i == sel {
                theme::TEXT
            } else if *enabled {
                theme::DIM
            } else {
                theme::FAINT
            };
            if i == sel {
                let w = self.fonts.measure(size, item) + 44.0;
                fb.fill_rrect(
                    ((WIDTH as f32 - w) / 2.0) as i32,
                    (y - size) as i32,
                    w as i32,
                    (size * 1.5) as i32,
                    6,
                    theme::PANEL,
                );
                self.fonts.draw_centered(
                    fb,
                    WIDTH as f32 / 2.0 - w / 2.0 + 8.0,
                    y,
                    size,
                    theme::TOP,
                    false,
                    "▸",
                );
            }
            self.fonts
                .draw_centered(fb, WIDTH as f32 / 2.0, y, size, color, i == sel, item);
        }
    }

    fn render_title(&mut self, fb: &mut Frame) {
        let Screen::Title { sel } = self.screen else {
            return;
        };
        fb.clear(theme::BG);
        self.draw_logo(fb, 108.0, 64.0);
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            140.0,
            17.0,
            theme::FAINT,
            false,
            "two players adversarially build a valuation",
        );
        let items: Vec<(String, bool)> = self
            .title_items()
            .into_iter()
            .map(|it| (self.title_label(it), true))
            .collect();
        // With the online item the list grows to eight rows: tighten up.
        let (y0, size) = if items.len() >= 8 {
            (188.0, 20.0)
        } else {
            (200.0, 22.0)
        };
        self.draw_menu(fb, &items, sel, y0, size);
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            HEIGHT as f32 - 16.0,
            15.0,
            theme::FAINT,
            false,
            "↑↓ choose · A confirm",
        );
    }

    fn render_rules(&mut self, fb: &mut Frame) {
        fb.clear(theme::BG);
        let size = 19.0;
        let lh = 21.0;
        let x = 28.0;
        for (i, line) in RULES_CARD.iter().enumerate() {
            let color = if i == 0 {
                theme::TOP
            } else if line.starts_with("SETUP")
                || line.starts_with("TURN")
                || line.starts_with("LAWS")
                || line.starts_with("WIN")
            {
                theme::TEXT
            } else {
                theme::DIM
            };
            self.fonts
                .draw(fb, x, 30.0 + i as f32 * lh, size, color, i == 0, line);
        }
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            HEIGHT as f32 - 8.0,
            15.0,
            theme::FAINT,
            false,
            "START to return",
        );
        self.tap_anywhere(Button::Start);
    }

    fn render_difficulty(&mut self, fb: &mut Frame) {
        let Screen::Difficulty { sel, target } = self.screen else {
            return;
        };
        fb.clear(theme::BG);
        let title = match target {
            DiffTarget::Duel => "duel — board size",
            DiffTarget::Online => "online duel — board size",
            DiffTarget::Adversary => "adversary — board size",
        };
        self.fonts
            .draw_centered(fb, WIDTH as f32 / 2.0, 90.0, 26.0, theme::TEXT, true, title);
        let items: Vec<(String, bool)> = [
            "1 · four atoms, ∧ ∨",
            "2 · four atoms, ∧ ∨ ⇒",
            "3 · five atoms, ∧ ∨ ⇒ ¬",
            "4 · six atoms, ∧ ∨ ⇒ = ¬",
            "5 · eight atoms, the full mix",
        ]
        .iter()
        .map(|s| (s.to_string(), true))
        .collect();
        self.draw_menu(fb, &items, sel, 170.0, 21.0);
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            HEIGHT as f32 - 16.0,
            15.0,
            theme::FAINT,
            false,
            "difficulty scales by the formula, never by blunders — B back",
        );
    }

    fn render_puzzles(&mut self, fb: &mut Frame) {
        let Screen::PuzzleList { sel } = self.screen else {
            return;
        };
        fb.clear(theme::BG);
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            60.0,
            26.0,
            theme::TEXT,
            true,
            "puzzles",
        );
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            86.0,
            15.0,
            theme::FAINT,
            false,
            "forced wins — exactly tsumego · ghost preview disabled",
        );
        let visible = 9usize;
        let n = self.puzzles.len();
        let first = sel
            .saturating_sub(visible - 1)
            .min(n.saturating_sub(visible));
        let items: Vec<(String, bool)> = self
            .puzzles
            .iter()
            .enumerate()
            .skip(first)
            .take(visible)
            .map(|(i, p)| {
                let solved = self.save.solved & (1 << i) != 0;
                let mark = if solved { "✓ " } else { "  " };
                (format!("{}{}", mark, p.title), true)
            })
            .collect();
        self.draw_menu(fb, &items, sel - first, 130.0, 19.0);
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            HEIGHT as f32 - 16.0,
            15.0,
            theme::FAINT,
            false,
            "A play · B back",
        );
    }

    fn render_gauntlet_menu(&mut self, fb: &mut Frame) {
        let Screen::GauntletMenu { sel } = self.screen else {
            return;
        };
        fb.clear(theme::BG);
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            100.0,
            26.0,
            theme::TEXT,
            true,
            "daily gauntlet",
        );
        let code = share_code(gauntlet_seed(&self.date_iso));
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            140.0,
            18.0,
            theme::DIM,
            false,
            &format!("{} · {}", self.date_iso, code),
        );
        let items: Vec<(String, bool)> = vec![
            ("play five seeded formulas".into(), true),
            ("enter a shared code".into(), true),
        ];
        self.draw_menu(fb, &items, sel, 220.0, 21.0);
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            HEIGHT as f32 - 16.0,
            15.0,
            theme::FAINT,
            false,
            "five formulas, difficulty 1→5 · share the code, compare scores",
        );
    }

    fn render_code_entry(&mut self, fb: &mut Frame) {
        let Screen::CodeEntry { chars, pos } = self.screen else {
            return;
        };
        fb.clear(theme::BG);
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            120.0,
            26.0,
            theme::TEXT,
            true,
            "enter code",
        );
        let size = 40.0;
        let cell = 52.0;
        let total = cell * 7.0;
        let x0 = (WIDTH as f32 - total) / 2.0;
        self.fonts
            .draw_centered(fb, x0 - 50.0, 250.0, 24.0, theme::FAINT, false, "TPL-");
        for i in 0..7 {
            let x = x0 + i as f32 * cell;
            let selected = i == pos;
            // Tap a cell to select it; tap the selected cell to bump it.
            let act = if selected {
                vec![Button::Up]
            } else if i > pos {
                vec![Button::Right; i - pos]
            } else {
                vec![Button::Left; pos - i]
            };
            self.tap_zone(x, 190.0, cell - 8.0, 104.0, TapAction::Press(act));
            fb.fill_rrect(
                x as i32,
                210,
                (cell - 8.0) as i32,
                64,
                6,
                if selected {
                    theme::PANEL_EDGE
                } else {
                    theme::PANEL
                },
            );
            if selected {
                fb.rect_outline(x as i32, 210, (cell - 8.0) as i32, 64, 2, theme::TOP);
            }
            self.fonts.draw_centered(
                fb,
                x + (cell - 8.0) / 2.0,
                258.0,
                size,
                theme::TEXT,
                selected,
                &(B32[chars[i]] as char).to_string(),
            );
        }
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            HEIGHT as f32 - 16.0,
            15.0,
            theme::FAINT,
            false,
            "↑↓ change · ◂ ▸ move · A play · B back",
        );
    }

    fn render_formula_panel(&mut self, fb: &mut Frame, title: &str) {
        fb.clear(theme::BG);
        self.fonts
            .draw_centered(fb, WIDTH as f32 / 2.0, 56.0, 22.0, theme::DIM, false, title);
        if let Some(p) = &self.pending {
            let layout = crate::layout::layout_formula(
                &mut self.fonts,
                &p.deal.f,
                None,
                WIDTH as f32 - 48.0,
                WIDTH as f32 / 2.0,
                180.0,
                3,
            );
            for g in &layout.glyphs {
                let color = match g.ch {
                    '⊤' => theme::TOP,
                    '⊥' => theme::BOT,
                    '(' | ')' => theme::FAINT,
                    '∧' | '∨' | '⇒' | '=' | '¬' => theme::DIM,
                    _ => theme::TEXT,
                };
                self.fonts
                    .draw_char(fb, g.x, g.y_baseline, layout.size, color, false, g.ch);
            }
        }
    }

    fn render_pick_side(&mut self, fb: &mut Frame) {
        let picker = match &self.mode {
            Some(Mode::Duel { round_no, .. }) => {
                if round_no % 2 == 1 {
                    "player 1 — price the formula, pick a side"
                } else {
                    "player 2 — price the formula, pick a side"
                }
            }
            Some(Mode::Online(_)) => "online duel — price the formula, pick a side",
            _ => "price the formula, pick a side",
        };
        self.render_formula_panel(fb, picker);
        // The two sides, as buttons.
        let cy = 320.0;
        for (i, (glyph, name, color, key)) in [
            ('⊤', "top", theme::TOP, "X"),
            ('⊥', "bottom", theme::BOT, "B"),
        ]
        .iter()
        .enumerate()
        {
            let cx = WIDTH as f32 / 2.0 + if i == 0 { -110.0 } else { 110.0 };
            let side_btn = if i == 0 { Button::X } else { Button::B };
            self.tap_zone(
                cx - 62.0,
                cy - 44.0,
                124.0,
                96.0,
                TapAction::Press(vec![side_btn]),
            );
            fb.fill_rrect(cx as i32 - 62, cy as i32 - 44, 124, 96, 10, theme::PANEL);
            fb.rect_outline(
                cx as i32 - 62,
                cy as i32 - 44,
                124,
                96,
                1,
                theme::PANEL_EDGE,
            );
            self.fonts
                .draw_centered(fb, cx, cy, 42.0, *color, true, &glyph.to_string());
            self.fonts.draw_centered(
                fb,
                cx,
                cy + 34.0,
                16.0,
                theme::DIM,
                false,
                &format!("{key} · {name}"),
            );
        }
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            HEIGHT as f32 - 16.0,
            15.0,
            theme::FAINT,
            false,
            "the other picker chooses who assigns first",
        );
    }

    fn render_pick_order(&mut self, fb: &mut Frame) {
        let Screen::PickOrder { sel } = self.screen else {
            return;
        };
        let (title, labels): (&str, [String; 2]) = if matches!(self.mode, Some(Mode::Online(_))) {
            let p1s = self
                .pending
                .as_ref()
                .and_then(|p| p.picked_side)
                .unwrap_or(Side::Top);
            let you = p1s.other();
            (
                "choose the tempo — who assigns first?",
                [
                    format!("you assign first ({})", you.glyph()),
                    format!("the opponent assigns first ({})", p1s.glyph()),
                ],
            )
        } else if self.vs_ai() {
            let ai = self
                .pending
                .as_ref()
                .and_then(|p| p.ai_side)
                .unwrap_or(Side::Top);
            (
                "choose the tempo — who assigns first?",
                [
                    format!("you assign first ({})", ai.other().glyph()),
                    format!("the Adversary assigns first ({})", ai.glyph()),
                ],
            )
        } else {
            let round_no = match &self.mode {
                Some(Mode::Duel { round_no, .. }) => *round_no,
                _ => 1,
            };
            let picker = if round_no % 2 == 1 { 2 } else { 1 };
            (
                if picker == 2 {
                    "player 2 — who assigns first?"
                } else {
                    "player 1 — who assigns first?"
                },
                [
                    "player 1 assigns first".to_string(),
                    "player 2 assigns first".to_string(),
                ],
            )
        };
        self.render_formula_panel(fb, title);
        let items: Vec<(String, bool)> = labels.iter().map(|l| (l.clone(), true)).collect();
        self.draw_menu(fb, &items, sel, 305.0, 20.0);
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            HEIGHT as f32 - 16.0,
            15.0,
            theme::FAINT,
            false,
            "parity matters: count the atoms · A confirm",
        );
    }

    fn render_notice(&mut self, fb: &mut Frame) {
        let (title, lines) = match &self.screen {
            Screen::Notice { title, lines, .. } => (title.clone(), lines.clone()),
            _ => return,
        };
        self.render_formula_panel(fb, &title);
        for (i, line) in lines.iter().enumerate() {
            self.fonts.draw_centered(
                fb,
                WIDTH as f32 / 2.0,
                305.0 + i as f32 * 30.0,
                20.0,
                theme::TEXT,
                false,
                line,
            );
        }
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            HEIGHT as f32 - 16.0,
            15.0,
            theme::FAINT,
            false,
            "A continue",
        );
        self.tap_anywhere(Button::A);
    }

    fn render_pause(&mut self, fb: &mut Frame) {
        let Screen::Pause { sel } = self.screen else {
            return;
        };
        if let Some(r) = &mut self.round {
            r.render(fb, &mut self.fonts, &mut self.taps);
        }
        // The overlay owns the screen: drop the board's zones.
        self.taps.clear();
        fb.fill_rect(0, 0, WIDTH as i32, HEIGHT as i32, theme::BG.with_alpha(200));
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            130.0,
            26.0,
            theme::TEXT,
            true,
            "paused",
        );
        let items: Vec<(String, bool)> = if matches!(self.mode, Some(Mode::Online(_))) {
            vec![
                ("resume".into(), true),
                ("rules".into(), true),
                ("leave — the match keeps waiting".into(), true),
                ("resign the match".into(), true),
            ]
        } else {
            vec![
                ("resume".into(), true),
                ("rules".into(), true),
                ("restart round".into(), true),
                ("quit to menu".into(), true),
            ]
        };
        self.draw_menu(fb, &items, sel, 200.0, 21.0);
    }

    fn render_online_wait(&mut self, fb: &mut Frame) {
        let (title, lines) = match &self.screen {
            Screen::OnlineWait { title, lines } => (title.clone(), lines.clone()),
            _ => return,
        };
        self.render_formula_panel(fb, &title);
        for (i, line) in lines.iter().enumerate() {
            self.fonts.draw_centered(
                fb,
                WIDTH as f32 / 2.0,
                305.0 + i as f32 * 30.0,
                20.0,
                theme::TEXT,
                false,
                line,
            );
        }
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            HEIGHT as f32 - 16.0,
            15.0,
            theme::FAINT,
            false,
            "B · back to the menu — the match keeps waiting",
        );
    }

    fn render_round_over(&mut self, fb: &mut Frame) {
        let (sel, title, win) = match &self.screen {
            Screen::RoundOver { sel, title, win } => (*sel, title.clone(), *win),
            _ => return,
        };
        if let Some(r) = &mut self.round {
            r.render(fb, &mut self.fonts, &mut self.taps);
        }
        // The overlay owns the screen: drop the board's zones.
        self.taps.clear();
        fb.fill_rect(0, 0, WIDTH as i32, HEIGHT as i32, theme::BG.with_alpha(210));
        let color = if title.starts_with('⊤') {
            theme::TOP
        } else if title.starts_with('⊥') {
            theme::BOT
        } else if win {
            theme::GOOD
        } else {
            theme::BAD
        };
        self.fonts
            .draw_centered(fb, WIDTH as f32 / 2.0, 120.0, 34.0, color, true, &title);
        if let Some(sub) = self.round_over_subtitle() {
            self.fonts
                .draw_centered(fb, WIDTH as f32 / 2.0, 152.0, 17.0, theme::DIM, false, &sub);
        }
        let items: Vec<(String, bool)> = self
            .round_over_items()
            .into_iter()
            .map(|s| (s, true))
            .collect();
        self.draw_menu(fb, &items, sel, 220.0, 21.0);
    }

    fn round_over_subtitle(&self) -> Option<String> {
        match &self.mode {
            Some(Mode::Adversary { you, adv, .. }) => {
                if *you >= 3 {
                    Some(format!("match over — you take it {you}–{adv}"))
                } else if *adv >= 3 {
                    Some(format!("match over — the Adversary takes it {adv}–{you}"))
                } else {
                    Some(format!("match to three · you {you} — {adv} adversary"))
                }
            }
            Some(Mode::Duel { score, .. }) => {
                Some(format!("session · P1 {} — {} P2", score[0], score[1]))
            }
            Some(Mode::Puzzle { index }) => {
                let pz = &self.puzzles[*index];
                Some(format!("book line: {} in {}", pz.you.glyph(), pz.mate_in))
            }
            Some(Mode::Gauntlet { results, .. }) => Some(format!(
                "gauntlet · {} of {} so far",
                results.iter().filter(|&&r| r).count(),
                results.len()
            )),
            Some(Mode::Online(o)) => {
                let p1s = o.st.p1_side()?;
                let local = if o.local_p1 { p1s } else { p1s.other() };
                Some(format!("online duel · you were {}", local.glyph()))
            }
            None => None,
        }
    }

    fn render_proof(&mut self, fb: &mut Frame) {
        let Screen::Proof { scroll } = self.screen else {
            return;
        };
        fb.clear(theme::BG);
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            40.0,
            22.0,
            theme::TEXT,
            true,
            "the proof",
        );
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            64.0,
            15.0,
            theme::FAINT,
            false,
            "a finished round's move list is verbatim an equational derivation",
        );
        let mut lines: Vec<(String, crate::fb::Color)> = Vec::new();
        if let Some(r) = &self.round {
            if let Some(first) = r.history.first() {
                lines.push((format!("      {}", pretty(&first.before)), theme::TEXT));
            }
            for (i, rec) in r.history.iter().enumerate() {
                lines.push((String::new(), theme::TEXT));
                lines.push((
                    format!(
                        "{}. {}  {} ≔ {}",
                        i + 1,
                        rec.mover.glyph(),
                        atom_name(rec.mv.atom),
                        if rec.mv.value { '⊤' } else { '⊥' }
                    ),
                    theme::side_color(rec.mover),
                ));
                for s in &rec.steps {
                    lines.push((
                        format!("      {:<16} ⊢  {}", s.law.equation(), pretty(&s.after)),
                        theme::DIM,
                    ));
                }
            }
        }
        let lh = 22.0;
        let visible = ((HEIGHT as f32 - 120.0) / lh) as i32;
        let max_scroll = (lines.len() as i32 - visible).max(0);
        let scroll = scroll.clamp(0, max_scroll);
        if let Screen::Proof { scroll: s } = &mut self.screen {
            *s = scroll;
        }
        for (i, (text, color)) in lines
            .iter()
            .enumerate()
            .skip(scroll as usize)
            .take(visible as usize)
        {
            self.fonts.draw(
                fb,
                28.0,
                96.0 + (i as i32 - scroll) as f32 * lh,
                16.0,
                *color,
                false,
                text,
            );
        }
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            HEIGHT as f32 - 14.0,
            15.0,
            theme::FAINT,
            false,
            "↑↓ scroll · B back",
        );
        // Tap the top or bottom half to scroll.
        let half = HEIGHT as f32 / 2.0;
        self.tap_zone(
            0.0,
            0.0,
            WIDTH as f32,
            half,
            TapAction::Press(vec![Button::Up]),
        );
        self.tap_zone(
            0.0,
            half,
            WIDTH as f32,
            half,
            TapAction::Press(vec![Button::Down]),
        );
    }

    fn render_gauntlet_summary(&mut self, fb: &mut Frame) {
        fb.clear(theme::BG);
        let Some(Mode::Gauntlet { seed, results, .. }) = &self.mode else {
            return;
        };
        let code = share_code(*seed);
        let score = results.iter().filter(|&&r| r).count();
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            110.0,
            28.0,
            theme::TEXT,
            true,
            "gauntlet complete",
        );
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            170.0,
            48.0,
            if score >= 3 { theme::GOOD } else { theme::BAD },
            true,
            &format!("{score} / 5"),
        );
        let marks: String = results
            .iter()
            .map(|&r| if r { '●' } else { '○' })
            .collect::<Vec<char>>()
            .iter()
            .map(|c| format!("{c} "))
            .collect();
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            220.0,
            26.0,
            theme::DIM,
            false,
            marks.trim(),
        );
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            280.0,
            20.0,
            theme::TOP,
            false,
            &code,
        );
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            310.0,
            16.0,
            theme::FAINT,
            false,
            "share the code — same five formulas, same order",
        );
        self.fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            HEIGHT as f32 - 16.0,
            15.0,
            theme::FAINT,
            false,
            "A menu",
        );
        self.tap_anywhere(Button::A);
    }
}
