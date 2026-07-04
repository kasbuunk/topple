// C ABI of libtopple_ios.a (crates/topple-ios). Keep in sync with
// crates/topple-ios/src/lib.rs. All calls must come from the main thread.
#ifndef TOPPLE_H
#define TOPPLE_H

#include <stdint.h>

// Boot the app. Date parts are the user's local date; `online` shows the
// "online duel" title item (Game Center signed in).
void topple_boot(uint32_t seed_lo, uint32_t seed_hi, uint32_t year,
                 uint32_t month, uint32_t day, uint32_t online);
void topple_set_online_available(uint32_t online);

// RGBA framebuffer, width*height*4 bytes, valid until the next boot.
const uint8_t *topple_fb_ptr(void);
uint32_t topple_fb_width(void);
uint32_t topple_fb_height(void);

// Advance the simulation and repaint the framebuffer.
void topple_frame(uint32_t dt_ms);

// Buttons: 0 Up, 1 Down, 2 Left, 3 Right, 4 A, 5 B, 6 X, 7 Y, 8 Start,
// 9 Select. `down` is 1 for press, 0 for release.
void topple_key(uint32_t code, uint32_t down);

// A tap in framebuffer coordinates (letterbox-corrected).
void topple_tap(float x, float y);

// Save blob polling: nonzero length means fresh bytes at topple_save_ptr().
uint32_t topple_save_poll(void);
const uint8_t *topple_save_ptr(void);

// Byte inbox: alloc, memcpy in, then commit as a save or as match data.
uint8_t *topple_inbox_alloc(uint32_t len);
void topple_inbox_load_save(void);

// Online duels (Game Center turn-based). One blob = the whole match.
uint32_t topple_online_request_poll(void); // 1–5 difficulty, 0 = none
uint32_t topple_online_resign_poll(void);  // 1 = player resigned
void topple_online_create(uint32_t seed_lo, uint32_t seed_hi, uint32_t level,
                          uint32_t local_p1);
uint32_t topple_online_load(uint32_t local_p1); // consumes inbox; 1 = ok
uint32_t topple_online_outbox_poll(void);
const uint8_t *topple_online_outbox_ptr(void);
// 0 no match, 1 local turn, 2 remote turn, 3 local won, 4 remote won.
uint32_t topple_online_status(void);
void topple_online_opponent_quit(void);

#endif
