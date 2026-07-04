//! ⊤OPP⊥E, the playable app: a pure state machine that eats buttons and
//! milliseconds and emits a 640×480 RGBA framebuffer. Every platform shim —
//! Miyoo framebuffer, browser canvas, desktop window — is a thin loop around
//! this crate.

pub mod app;
pub mod fb;
pub mod font;
pub mod input;
pub mod layout;
pub mod round;
pub mod save;
pub mod theme;

pub use app::App;
pub use fb::{Frame, HEIGHT, WIDTH};
pub use input::Button;

#[cfg(test)]
mod tests {
    use super::*;

    fn boot() -> (App, Frame) {
        (App::new(0xBEEF, "2026-07-03"), Frame::new())
    }

    fn press(app: &mut App, b: Button) {
        app.on_press(b);
        app.on_release(b);
        app.tick(16);
    }

    /// Drive the app long enough for animations/AI to settle.
    fn settle(app: &mut App, ms: u32) {
        let mut left = ms;
        while left > 0 {
            app.tick(50);
            left = left.saturating_sub(50);
        }
    }

    #[test]
    fn boots_to_title_and_renders() {
        let (mut app, mut fb) = boot();
        app.render(&mut fb);
        // Something was drawn (not all background).
        let bg = fb.px.chunks_exact(4).filter(|p| p[0] == 0x0E).count();
        assert!(bg < fb::WIDTH * fb::HEIGHT, "title screen is blank");
    }

    #[test]
    fn rules_screen_opens_and_returns() {
        let (mut app, mut fb) = boot();
        for _ in 0..4 {
            press(&mut app, Button::Down);
        }
        press(&mut app, Button::A); // rules
        app.render(&mut fb);
        press(&mut app, Button::Start); // back
        app.render(&mut fb);
    }

    #[test]
    fn full_duel_round_plays_to_a_win() {
        let (mut app, mut fb) = boot();
        press(&mut app, Button::A); // duel
        app.render(&mut fb);
        press(&mut app, Button::A); // difficulty select (default)
        app.render(&mut fb);
        // Pie rule: P1 picks ⊤, P2 lets P1 go first.
        press(&mut app, Button::X);
        app.render(&mut fb);
        press(&mut app, Button::A);
        app.render(&mut fb);
        // Now in play. Assign every atom to ⊤ until the board collapses.
        for _ in 0..64 {
            press(&mut app, Button::X); // assign (or fast-forward anim)
            settle(&mut app, 8000);
            app.render(&mut fb);
        }
        // Round must have ended by now (menus accept A).
        press(&mut app, Button::A);
        app.render(&mut fb);
    }

    #[test]
    fn adversary_round_ai_moves_and_finishes() {
        let (mut app, mut fb) = boot();
        press(&mut app, Button::Down);
        press(&mut app, Button::A); // adversary
        app.render(&mut fb);
        press(&mut app, Button::A); // level
        app.render(&mut fb);
        press(&mut app, Button::X); // pick ⊤
        app.render(&mut fb);
        press(&mut app, Button::A); // notice: continue
        app.render(&mut fb);
        // Play out: keep assigning ⊥ as the human whenever it's our turn;
        // let the AI think between.
        for _ in 0..64 {
            settle(&mut app, 10_000);
            app.render(&mut fb);
            press(&mut app, Button::B);
            app.render(&mut fb);
        }
        press(&mut app, Button::A);
        app.render(&mut fb);
    }

    #[test]
    fn puzzle_solves_with_the_book_move() {
        let (mut app, mut fb) = boot();
        // Navigate: down x2 to puzzles.
        press(&mut app, Button::Down);
        press(&mut app, Button::Down);
        press(&mut app, Button::A);
        app.render(&mut fb);
        press(&mut app, Button::A); // first puzzle: the worked example
        app.render(&mut fb);
        // Board: (p ⇒ q) ∧ (p ∨ r) ∧ (r ⇒ q); cursor starts on p.
        // Move to q (second occurrence in reading order) and set ⊤.
        press(&mut app, Button::Right);
        app.render(&mut fb);
        press(&mut app, Button::X); // q ≔ ⊤
        settle(&mut app, 20_000); // cascade + adversary reply
        app.render(&mut fb);
        // Adversary deletes a prong; we take the other. The remaining board
        // is a single atom (p or r) — either occurrence, assign ⊤.
        press(&mut app, Button::X);
        settle(&mut app, 20_000);
        app.render(&mut fb);
        // Should be solved: save blob records it.
        let blob = app.take_save().expect("puzzle solve must dirty the save");
        let data = crate::save::SaveData::from_bytes(&blob).unwrap();
        assert!(data.solved & 1 != 0, "puzzle 0 not marked solved");
    }

    #[test]
    fn full_gauntlet_reaches_the_summary() {
        // Drive all five rounds with blind button-mashing: X picks a side or
        // assigns ⊤, A confirms menus, B assigns ⊥. The mode flow must pass
        // through the summary and land back on the title.
        let (mut app, mut fb) = boot();
        for b in [Button::Down, Button::Down, Button::Down] {
            press(&mut app, b);
        }
        press(&mut app, Button::A); // gauntlet menu
        press(&mut app, Button::A); // play today's
        let mut seen_summary = false;
        'outer: for _ in 0..400 {
            for b in [Button::X, Button::A, Button::B] {
                press(&mut app, b);
                settle(&mut app, 6000);
                app.render(&mut fb);
                if app.screen_name() == "gauntlet-summary" {
                    seen_summary = true;
                    break 'outer;
                }
            }
        }
        assert!(seen_summary, "never reached the gauntlet summary");
        press(&mut app, Button::A);
        assert_eq!(app.screen_name(), "title");
    }

    #[test]
    fn adversary_tempo_round_flow() {
        // Round 2 of an adversary match is the pick-the-tempo round:
        // notice → pick-order → play. Win or lose round 1 first.
        let (mut app, mut fb) = boot();
        press(&mut app, Button::Down);
        press(&mut app, Button::A); // adversary
        press(&mut app, Button::A); // level
        assert_eq!(app.screen_name(), "pick-side");
        press(&mut app, Button::X); // take ⊤
        assert_eq!(app.screen_name(), "notice");
        press(&mut app, Button::A); // continue → play
        assert_eq!(app.screen_name(), "play");
        // Mash until round 1 ends and we advance into round 2.
        for _ in 0..100 {
            if app.screen_name() == "round-over" {
                break;
            }
            press(&mut app, Button::B);
            settle(&mut app, 6000);
            app.render(&mut fb);
        }
        assert_eq!(app.screen_name(), "round-over");
        press(&mut app, Button::A); // next round
        // Even round: the Adversary announces its side, we pick the order.
        assert_eq!(app.screen_name(), "notice");
        press(&mut app, Button::A);
        assert_eq!(app.screen_name(), "pick-order");
        press(&mut app, Button::A);
        assert_eq!(app.screen_name(), "play");
    }

    #[test]
    fn gauntlet_menu_shows_todays_code() {
        let (mut app, mut fb) = boot();
        press(&mut app, Button::Down);
        press(&mut app, Button::Down);
        press(&mut app, Button::Down);
        press(&mut app, Button::A); // gauntlet menu
        app.render(&mut fb);
        press(&mut app, Button::B); // back out without dealing
        app.render(&mut fb);
    }

    #[test]
    fn strict_mode_toggles_and_saves() {
        let (mut app, _fb) = boot();
        for _ in 0..5 {
            press(&mut app, Button::Down);
        }
        press(&mut app, Button::A); // toggle strict
        let blob = app.take_save().expect("toggle must dirty save");
        let data = crate::save::SaveData::from_bytes(&blob).unwrap();
        assert!(data.strict);
    }
}
