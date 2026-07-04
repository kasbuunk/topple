//! One round of ⊤OPP⊥E: a board, two players, the cascade animation.
//! The pie-rule setup happens in the app's screens; a `Round` starts when
//! sides and move order are already settled.

use crate::fb::{Frame, HEIGHT, WIDTH};
use crate::font::FontEngine;
use crate::input::Button;
use crate::layout::{layout_formula, Layout};
use crate::theme;
use topple_core::{
    apply_move, atom_name, pretty, winner, Atom, Move, Path, Rng, Side, Solver, Step, F,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerKind {
    /// Human with a pass-and-play label (1 or 2).
    Human(u8),
    Adversary,
}

impl PlayerKind {
    pub fn is_human(self) -> bool {
        matches!(self, PlayerKind::Human(_))
    }
}

#[derive(Clone, Debug)]
pub struct Record {
    pub mover: Side,
    pub mv: Move,
    pub before: F,
    pub steps: Vec<Step>,
}

#[derive(Clone, Debug)]
enum Highlight {
    None,
    AtomAll(Atom),
    Redex(Path),
}

struct AnimFrame {
    board: F,
    highlight: Highlight,
    toast: Option<(String, bool)>, // text, is_law (laws get the amber box)
    dur: u32,
}

struct Anim {
    frames: Vec<AnimFrame>,
    idx: usize,
    t: u32,
}

const SUBST_MS: u32 = 700;
const LAW_MS: u32 = 850;
const FINAL_MS: u32 = 300;
const AI_DELAY_MS: u32 = 700;

pub struct RoundCfg {
    /// Ghost preview allowed (off in Strict duels and all puzzles).
    pub allow_preview: bool,
    /// Top-left label, e.g. "DUEL · ROUND 2".
    pub label: String,
    /// Optional top-right status (scores).
    pub status: String,
}

pub struct Round {
    pub board: F,
    pub to_move: Side,
    pub top_player: PlayerKind,
    pub bot_player: PlayerKind,
    pub cfg: RoundCfg,
    pub history: Vec<Record>,
    pub outcome: Option<Side>,
    cursor: usize,
    zoom: Option<Path>,
    preview: bool,
    anim: Option<Anim>,
    ai_timer: u32,
    solver: Solver,
    last_layout: Option<Layout>,
}

impl Round {
    pub fn new(
        board: F,
        first: Side,
        top_player: PlayerKind,
        bot_player: PlayerKind,
        cfg: RoundCfg,
    ) -> Round {
        Round {
            board,
            to_move: first,
            top_player,
            bot_player,
            cfg,
            history: Vec::new(),
            outcome: None,
            cursor: 0,
            zoom: None,
            preview: false,
            anim: None,
            ai_timer: 0,
            solver: Solver::new(),
            last_layout: None,
        }
    }

    pub fn player(&self, s: Side) -> PlayerKind {
        match s {
            Side::Top => self.top_player,
            Side::Bot => self.bot_player,
        }
    }

    pub fn mover_is_human(&self) -> bool {
        self.player(self.to_move).is_human()
    }

    pub fn animating(&self) -> bool {
        self.anim.is_some()
    }

    /// The atom under the cursor, if any.
    pub fn hovered_atom(&self) -> Option<Atom> {
        let layout = self.last_layout.as_ref()?;
        let occs = layout.occurrences();
        occs.get(self.cursor).and_then(|&gi| layout.glyphs[gi].atom)
    }

    // ------------------------------------------------------------- input --

    /// Make sure the cursor has a board to walk: input can arrive before the
    /// first render of a fresh board.
    fn ensure_layout(&mut self, fonts: &mut FontEngine) {
        if self.last_layout.is_none() {
            self.last_layout = Some(layout_formula(
                fonts,
                &self.board,
                self.zoom.as_ref(),
                WIDTH as f32 - 48.0,
                WIDTH as f32 / 2.0,
                212.0,
                3,
            ));
        }
    }

    /// Handle a button during play. Returns true if the button was consumed.
    pub fn press(&mut self, b: Button, rng: &mut Rng, fonts: &mut FontEngine) -> bool {
        self.ensure_layout(fonts);
        if self.anim.is_some() {
            // Any face button fast-forwards the cascade.
            if matches!(b, Button::A | Button::B | Button::X | Button::Y) {
                self.finish_anim();
                return true;
            }
            return false;
        }
        if self.outcome.is_some() || !self.mover_is_human() {
            return false;
        }
        match b {
            Button::Left => self.move_cursor(-1),
            Button::Right => self.move_cursor(1),
            Button::Up => self.move_cursor_line(-1),
            Button::Down => self.move_cursor_line(1),
            Button::X => self.try_assign(true, rng),
            Button::B => self.try_assign(false, rng),
            Button::Y => {
                if self.cfg.allow_preview {
                    self.preview = !self.preview;
                }
            }
            Button::A => self.toggle_zoom(),
            _ => return false,
        }
        true
    }

    fn move_cursor(&mut self, delta: i32) {
        let Some(layout) = &self.last_layout else {
            return;
        };
        let n = layout.occurrences().len();
        if n == 0 {
            return;
        }
        let cur = self.cursor.min(n - 1) as i32;
        self.cursor = (cur + delta).rem_euclid(n as i32) as usize;
    }

    fn move_cursor_line(&mut self, dir: i32) {
        let Some(layout) = &self.last_layout else {
            return;
        };
        let occs = layout.occurrences();
        if occs.is_empty() {
            return;
        }
        let cur_gi = occs[self.cursor.min(occs.len() - 1)];
        let (cl, cx) = (layout.glyphs[cur_gi].line as i32, layout.glyphs[cur_gi].x);
        // Nearest occurrence on a line strictly in direction `dir`.
        let mut best: Option<(i32, f32, usize)> = None;
        for (oi, &gi) in occs.iter().enumerate() {
            let g = &layout.glyphs[gi];
            let dl = g.line as i32 - cl;
            if dl.signum() != dir.signum() || dl == 0 {
                continue;
            }
            let key = (dl.abs(), (g.x - cx).abs());
            if best.is_none_or(|(bl, bx, _)| key < (bl, bx)) {
                best = Some((key.0, key.1, oi));
            }
        }
        if let Some((_, _, oi)) = best {
            self.cursor = oi;
        }
    }

    fn toggle_zoom(&mut self) {
        if self.zoom.is_some() {
            self.zoom = None;
            self.cursor = 0;
            self.last_layout = None;
            return;
        }
        // Zoom to the smallest binary subformula strictly containing the
        // hovered atom (a pure reading aid on big boards).
        let Some(layout) = &self.last_layout else {
            return;
        };
        let occs = layout.occurrences();
        let Some(&gi) = occs.get(self.cursor.min(occs.len().saturating_sub(1))) else {
            return;
        };
        let mut path = layout.glyphs[gi].path.clone();
        while !path.is_empty() {
            path.pop();
            if let Some(F::Bin(..)) = self.board.at(&path) {
                if !path.is_empty() {
                    self.zoom = Some(path);
                    self.cursor = 0;
                    self.last_layout = None;
                }
                return;
            }
        }
    }

    fn try_assign(&mut self, value: bool, _rng: &mut Rng) {
        let Some(atom) = self.hovered_atom() else {
            return;
        };
        self.play_move(Move::new(atom, value), false);
    }

    // -------------------------------------------------------------- moves --

    fn play_move(&mut self, mv: Move, by_ai: bool) {
        let before = self.board.clone();
        let Ok((after, steps)) = apply_move(&before, mv) else {
            return;
        };
        let mover = self.to_move;
        self.history.push(Record {
            mover,
            mv,
            before: before.clone(),
            steps: steps.clone(),
        });

        // Build the animation timeline: assignment flash, then one law at a
        // time, each naming the equation that fired.
        let who = if by_ai { "Adversary: " } else { "" };
        let mut frames = vec![AnimFrame {
            board: before.clone(),
            highlight: Highlight::AtomAll(mv.atom),
            toast: Some((
                format!(
                    "{}{} ≔ {}",
                    who,
                    atom_name(mv.atom),
                    if mv.value { '⊤' } else { '⊥' }
                ),
                false,
            )),
            dur: SUBST_MS,
        }];
        let mut cur = before.substitute(mv.atom, mv.value);
        for s in &steps {
            frames.push(AnimFrame {
                board: cur.clone(),
                highlight: Highlight::Redex(s.path.clone()),
                toast: Some((s.law.equation().to_string(), true)),
                dur: LAW_MS,
            });
            cur = s.after.clone();
        }
        frames.push(AnimFrame {
            board: after.clone(),
            highlight: Highlight::None,
            toast: None,
            dur: FINAL_MS,
        });

        self.board = after;
        self.anim = Some(Anim {
            frames,
            idx: 0,
            t: 0,
        });
        self.zoom = None;
        self.preview = false;
        self.cursor = 0;
        self.ai_timer = 0;
        self.last_layout = None;
    }

    fn finish_anim(&mut self) {
        self.anim = None;
        self.after_move();
    }

    fn after_move(&mut self) {
        if let Some(w) = winner(&self.board) {
            self.outcome = Some(w);
        } else {
            self.to_move = self.to_move.other();
        }
    }

    // --------------------------------------------------------------- tick --

    pub fn tick(&mut self, dt: u32, rng: &mut Rng) {
        if let Some(anim) = &mut self.anim {
            anim.t += dt;
            while anim.idx < anim.frames.len() && anim.t >= anim.frames[anim.idx].dur {
                anim.t -= anim.frames[anim.idx].dur;
                anim.idx += 1;
            }
            if anim.idx >= anim.frames.len() {
                self.finish_anim();
            }
            return;
        }
        if self.outcome.is_some() || self.mover_is_human() {
            return;
        }
        // Adversary's turn: think for a beat, then play a perfect move.
        self.ai_timer += dt;
        if self.ai_timer >= AI_DELAY_MS {
            let best = self.solver.best_moves(&self.board, self.to_move);
            let mv = if best.is_empty() {
                // Lost position: drag it out (best_moves already drags, but
                // guard against the impossible).
                *rng.pick(&topple_core::legal_moves(&self.board))
            } else {
                *rng.pick(&best)
            };
            self.play_move(mv, true);
        }
    }

    // ------------------------------------------------------------- render --

    pub fn render(&mut self, fb: &mut Frame, fonts: &mut FontEngine) {
        fb.clear(theme::BG);

        // Top bar.
        fb.fill_rect(0, 0, WIDTH as i32, 44, theme::PANEL);
        fb.hline(0, 44, WIDTH as i32, theme::PANEL_EDGE);
        fonts.draw(fb, 16.0, 30.0, 20.0, theme::DIM, false, &self.cfg.label);
        let status_w = fonts.measure(20.0, &self.cfg.status);
        fonts.draw(
            fb,
            WIDTH as f32 - 16.0 - status_w,
            30.0,
            20.0,
            theme::DIM,
            false,
            &self.cfg.status,
        );

        // Turn / outcome banner.
        let (banner, color) = match self.outcome {
            Some(w) => (format!("{} wins", w.glyph()), theme::side_color(w)),
            None => {
                let who = match self.player(self.to_move) {
                    PlayerKind::Human(n) => format!("P{n} · "),
                    PlayerKind::Adversary => "Adversary · ".to_string(),
                };
                (
                    format!("{}{} to move", who, self.to_move.glyph()),
                    theme::side_color(self.to_move),
                )
            }
        };
        fonts.draw_centered(fb, WIDTH as f32 / 2.0, 78.0, 22.0, color, true, &banner);

        // The board (during animation, the anim frame's board).
        let (board, highlight, toast) = match &self.anim {
            Some(a) if a.idx < a.frames.len() => {
                let fr = &a.frames[a.idx];
                (fr.board.clone(), fr.highlight.clone(), fr.toast.clone())
            }
            _ => (self.board.clone(), Highlight::None, None),
        };

        let zoomed = self.anim.is_none().then_some(self.zoom.as_ref()).flatten();
        if zoomed.is_some() {
            fonts.draw_centered(
                fb,
                WIDTH as f32 / 2.0,
                108.0,
                16.0,
                theme::FAINT,
                false,
                "· zoomed — A to zoom out ·",
            );
        }
        let layout = layout_formula(
            fonts,
            &board,
            zoomed,
            WIDTH as f32 - 48.0,
            WIDTH as f32 / 2.0,
            212.0,
            3,
        );

        // Highlights under the glyphs.
        let hover = if self.anim.is_none() && self.outcome.is_none() && self.mover_is_human() {
            let occs = layout.occurrences();
            occs.get(self.cursor.min(occs.len().saturating_sub(1)))
                .copied()
        } else {
            None
        };
        let hover_atom = hover.and_then(|gi| layout.glyphs[gi].atom);

        let pad = 3.0;
        let cell_top = |g: &crate::layout::LaidGlyph| {
            (
                g.x - pad,
                g.y_baseline - fonts.ascent(layout.size) - pad,
                g.w + pad * 2.0,
                fonts.line_height(layout.size) + pad,
            )
        };
        match &highlight {
            Highlight::AtomAll(a) => {
                for g in &layout.glyphs {
                    if g.atom == Some(*a) {
                        let (x, y, w, h) = cell_top(g);
                        fb.fill_rrect(x as i32, y as i32, w as i32, h as i32, 4, theme::REDEX_BG);
                    }
                }
            }
            Highlight::Redex(p) => {
                for gi in layout.glyph_span_of_path(p) {
                    let g = &layout.glyphs[gi];
                    let (x, y, w, h) = cell_top(g);
                    fb.fill_rect(x as i32, y as i32, w as i32, h as i32, theme::REDEX_BG);
                }
            }
            Highlight::None => {}
        }
        // Hovered atom: every occurrence glows together (assignment is
        // global, and the glow is how the board teaches that).
        if let Some(ha) = hover_atom {
            for (gi, g) in layout.glyphs.iter().enumerate() {
                if g.atom == Some(ha) {
                    let (x, y, w, h) = cell_top(g);
                    fb.fill_rrect(x as i32, y as i32, w as i32, h as i32, 4, theme::GLOW_BG);
                    if Some(gi) == hover {
                        fb.rect_outline(x as i32, y as i32, w as i32, h as i32, 2, theme::CURSOR);
                    }
                }
            }
        }

        // Glyphs.
        for g in &layout.glyphs {
            let color = match g.ch {
                '⊤' => theme::TOP,
                '⊥' => theme::BOT,
                '(' | ')' => theme::FAINT,
                '∧' | '∨' | '⇒' | '=' | '¬' => theme::DIM,
                _ => {
                    if Some(g.ch) == hover_atom.map(atom_name) {
                        theme::CURSOR
                    } else {
                        theme::TEXT
                    }
                }
            };
            fonts.draw_char(
                fb,
                g.x,
                g.y_baseline,
                layout.size,
                color,
                g.atom.is_some(),
                g.ch,
            );
        }

        // Toast: the equation that is firing.
        if let Some((text, is_law)) = &toast {
            let w = fonts.measure(24.0, text) + 40.0;
            let x = (WIDTH as f32 - w) / 2.0;
            let y = 356;
            fb.fill_rrect(x as i32, y, w as i32, 46, 8, theme::PANEL);
            fb.rect_outline(
                x as i32,
                y,
                w as i32,
                46,
                1,
                if *is_law {
                    theme::REDEX_EDGE
                } else {
                    theme::PANEL_EDGE
                },
            );
            fonts.draw_centered(
                fb,
                WIDTH as f32 / 2.0,
                y as f32 + 32.0,
                24.0,
                theme::TEXT,
                false,
                text,
            );
        }

        // Ghost preview: where each assignment of the hovered atom lands.
        if self.preview && self.anim.is_none() {
            if let Some(a) = hover_atom {
                self.render_preview(fb, fonts, a);
            }
        }

        // Help bar.
        let help = if self.anim.is_some() {
            "any button: fast-forward".to_string()
        } else if self.outcome.is_some() {
            String::new()
        } else if self.mover_is_human() {
            let mut h = String::from("◂ ▸ atom   X ⊤   B ⊥");
            if self.cfg.allow_preview {
                h.push_str("   Y peek");
            }
            h.push_str("   A zoom   START menu");
            h
        } else {
            "the Adversary is reading the board…".to_string()
        };
        fonts.draw_centered(
            fb,
            WIDTH as f32 / 2.0,
            HEIGHT as f32 - 18.0,
            17.0,
            theme::FAINT,
            false,
            &help,
        );

        self.last_layout = Some(layout);
    }

    fn render_preview(&self, fb: &mut Frame, fonts: &mut FontEngine, atom: Atom) {
        let y0 = 392;
        fb.fill_rrect(24, y0, WIDTH as i32 - 48, 52, 6, theme::PANEL);
        fb.rect_outline(24, y0, WIDTH as i32 - 48, 52, 1, theme::PANEL_EDGE);
        for (i, val) in [true, false].into_iter().enumerate() {
            let (result, _) = apply_move(&self.board, Move::new(atom, val))
                .unwrap_or((self.board.clone(), Vec::new()));
            let mut text = format!(
                "{} ≔ {}  ▸  {}",
                atom_name(atom),
                if val { '⊤' } else { '⊥' },
                pretty(&result)
            );
            let max_w = WIDTH as f32 - 96.0;
            let size = 16.0;
            while fonts.measure(size, &text) > max_w && text.chars().count() > 8 {
                let cut: String = text.chars().take(text.chars().count() - 2).collect();
                text = format!("{cut}…");
            }
            let color = match winner(&result) {
                Some(w) => theme::side_color(w),
                None => theme::DIM,
            };
            fonts.draw(
                fb,
                40.0,
                y0 as f32 + 21.0 + i as f32 * 22.0,
                size,
                color,
                false,
                &text,
            );
        }
    }
}
