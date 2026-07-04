//! ⊤OPP⊥E, the playable app: a pure state machine that eats buttons and
//! milliseconds and emits a 640×480 RGBA framebuffer. Every platform shim —
//! Miyoo framebuffer, browser canvas, desktop window — is a thin loop around
//! this crate.

pub mod app;
pub mod fb;
pub mod font;
pub mod input;
pub mod layout;
pub mod online;
pub mod round;
pub mod save;
pub mod theme;

pub use app::{App, OnlineStatus};
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

    /// Ship any fresh outbox bytes from one app to the other.
    fn relay(from: &mut App, to: &mut App, to_is_p1: bool) -> bool {
        if let Some(blob) = from.take_online_outbox() {
            assert!(to.online_load(&blob, to_is_p1), "peer rejected the blob");
            return true;
        }
        false
    }

    #[test]
    fn online_duel_plays_across_two_apps() {
        let (mut a, _) = boot();
        let mut b = App::new(0xCAFE, "2026-07-03");
        a.configure_platform(true, false);
        b.configure_platform(true, false);

        // A asks for an online duel from the title menu.
        press(&mut a, Button::Down); // onto "online duel"
        press(&mut a, Button::A);
        assert_eq!(a.screen_name(), "difficulty");
        press(&mut a, Button::A); // confirm level 2 (default sel)
        assert_eq!(a.take_online_request(), Some(2));

        // The platform arranges a match; A is the creator (P1).
        a.online_create(77, 2, true);
        assert!(relay(&mut a, &mut b, false), "creator ships the header");
        assert_eq!(a.screen_name(), "pick-side");
        assert_eq!(b.screen_name(), "online-wait");
        assert_eq!(a.online_status(), OnlineStatus::LocalTurn);
        assert_eq!(b.online_status(), OnlineStatus::RemoteTurn);

        // A prices the formula and takes ⊤; B picks the tempo.
        press(&mut a, Button::X);
        assert_eq!(a.screen_name(), "online-wait");
        assert!(relay(&mut a, &mut b, false));
        assert_eq!(b.screen_name(), "pick-order");
        press(&mut b, Button::A); // B assigns first
        assert!(relay(&mut b, &mut a, true));
        assert_eq!(a.screen_name(), "play");
        assert_eq!(b.screen_name(), "play");

        // Play to the end: whoever is to act assigns ⊤ at the cursor.
        for _ in 0..64 {
            let (mover, waiter, mover_p1) = match a.online_status() {
                OnlineStatus::LocalTurn => (&mut a, &mut b, false),
                OnlineStatus::RemoteTurn => (&mut b, &mut a, true),
                _ => break,
            };
            press(mover, Button::X);
            settle(mover, 30_000);
            assert!(relay(mover, waiter, mover_p1), "a move must ship");
            settle(waiter, 30_000);
        }
        let (sa, sb) = (a.online_status(), b.online_status());
        assert!(
            matches!(
                (sa, sb),
                (OnlineStatus::WonLocal, OnlineStatus::WonRemote)
                    | (OnlineStatus::WonRemote, OnlineStatus::WonLocal)
            ),
            "match must end with mirrored outcomes, got {sa:?} / {sb:?}"
        );
        settle(&mut a, 5000);
        settle(&mut b, 5000);
        assert_eq!(a.screen_name(), "round-over");
        assert_eq!(b.screen_name(), "round-over");
    }

    #[test]
    fn online_match_rebuilds_after_relaunch() {
        let (mut a, _) = boot();
        let mut b = App::new(0xCAFE, "2026-07-03");
        a.configure_platform(true, false);
        b.configure_platform(true, false);
        a.online_create(123, 1, true);
        let hdr = a.take_online_outbox().unwrap();
        assert!(b.online_load(&hdr, false));
        press(&mut a, Button::B); // A takes ⊥
        relay(&mut a, &mut b, false);
        press(&mut b, Button::A); // B: P... local assigns first
        relay(&mut b, &mut a, true);
        // A makes one assignment.
        let mover_is_a = a.online_status() == OnlineStatus::LocalTurn;
        let (mover, waiter, waiter_p1) = if mover_is_a {
            (&mut a, &mut b, false)
        } else {
            (&mut b, &mut a, true)
        };
        press(mover, Button::X);
        settle(mover, 30_000);
        let blob = mover.take_online_outbox().unwrap();
        assert!(waiter.online_load(&blob, waiter_p1));

        // A brand-new app (relaunch) rebuilds the same position instantly.
        settle(waiter, 30_000);
        let mut fresh = App::new(1, "2026-07-04");
        fresh.configure_platform(true, false);
        assert!(fresh.online_load(&blob, waiter_p1));
        settle(&mut fresh, 30_000);
        assert_eq!(fresh.screen_name(), waiter.screen_name());
        assert_eq!(fresh.online_status(), waiter.online_status());
    }

    #[test]
    fn online_resign_and_opponent_quit() {
        let (mut a, _) = boot();
        let mut b = App::new(0xCAFE, "2026-07-03");
        a.configure_platform(true, false);
        b.configure_platform(true, false);
        a.online_create(9, 1, true);
        relay(&mut a, &mut b, false);
        press(&mut a, Button::X);
        relay(&mut a, &mut b, false);
        press(&mut b, Button::A);
        relay(&mut b, &mut a, true);

        // A resigns from the pause menu (resume/rules/leave/resign).
        press(&mut a, Button::Start);
        assert_eq!(a.screen_name(), "pause");
        for _ in 0..3 {
            press(&mut a, Button::Down);
        }
        press(&mut a, Button::A);
        assert!(a.take_online_resign());
        assert_eq!(a.screen_name(), "title");
        assert_eq!(a.online_status(), OnlineStatus::Inactive);

        // The platform tells B the opponent quit.
        b.online_opponent_quit();
        assert_eq!(b.screen_name(), "notice");
        press(&mut b, Button::A);
        assert_eq!(b.screen_name(), "title");
    }

    #[test]
    fn title_menu_grows_with_platform_items() {
        let (mut app, mut fb) = boot();
        app.configure_platform(true, false);
        // duel, online duel, adversary, puzzles, gauntlet, rules, strict — no quit.
        press(&mut app, Button::Up); // wraps to the last item (strict)
        press(&mut app, Button::A);
        assert!(app.save.strict, "last item without quit must be strict");
        app.render(&mut fb);
    }

    #[test]
    fn taps_drive_the_menus() {
        let (mut app, mut fb) = boot();
        app.render(&mut fb);
        // Tap the third title row ("puzzles") dead centre.
        let zones = app.tap_zones();
        assert!(zones.len() >= 7, "title rows must be tappable");
        let z = &zones[2];
        let (cx, cy) = (z.x + z.w / 2.0, z.y + z.h / 2.0);
        app.on_tap(cx, cy);
        assert_eq!(app.screen_name(), "puzzle-list");
    }

    #[test]
    fn taps_select_atoms_on_the_board() {
        use crate::input::TapAction;
        let (mut app, mut fb) = boot();
        press(&mut app, Button::Down);
        press(&mut app, Button::Down);
        press(&mut app, Button::A); // puzzles
        press(&mut app, Button::A); // first puzzle
        app.render(&mut fb);
        assert_eq!(app.screen_name(), "play");
        let before = app.round_hovered_atom();
        // Tap the last atom occurrence; the cursor must land on it.
        let cursor_zones: Vec<_> = app
            .tap_zones()
            .iter()
            .filter(|z| matches!(z.act, TapAction::Cursor(_)))
            .cloned()
            .collect();
        assert!(cursor_zones.len() >= 3, "atoms must be tappable");
        let z = cursor_zones.last().unwrap();
        app.on_tap(z.x + z.w / 2.0, z.y + z.h / 2.0);
        app.render(&mut fb);
        let after = app.round_hovered_atom();
        assert!(after.is_some());
        assert_ne!(before, after, "tapping a far atom moves the cursor");
    }

    #[test]
    fn tapping_a_side_card_picks_a_side() {
        let (mut app, mut fb) = boot();
        press(&mut app, Button::A); // duel
        press(&mut app, Button::A); // difficulty
        assert_eq!(app.screen_name(), "pick-side");
        app.render(&mut fb);
        // The two side cards are the topmost zones.
        let card = app
            .tap_zones()
            .iter()
            .rev()
            .find(|z| z.w < 200.0)
            .cloned()
            .expect("side cards registered");
        app.on_tap(card.x + card.w / 2.0, card.y + card.h / 2.0);
        assert_eq!(app.screen_name(), "pick-order");
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
