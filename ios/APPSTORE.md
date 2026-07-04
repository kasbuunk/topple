# Shipping Topple to the App Store

Everything that can be prepared without a Mac and an Apple Developer
account is in this repository. What remains is the part Apple gates behind
both. This is the map.

## What is already done

- `crates/topple-ios` — the whole game behind a C ABI, unit-tested; the
  static libraries cross-compile from any host:
  `scripts/build-ios.sh` produces
  `target/aarch64-apple-ios/release/libtopple_ios.a` (device) and the
  simulator slices.
- `ios/Topple` — the complete Swift app: framebuffer rendering, touch and
  tap-zone input, virtual pad buttons, save persistence, and online duels
  over Game Center turn-based matches (serverless, App Store–native).
- `ios/project.yml` — XcodeGen manifest that generates `Topple.xcodeproj`.
- Icon (1024, generated from the game's own renderer), launch screen,
  Info.plist (landscape-only), Game Center entitlement, privacy manifest,
  fastlane lanes and store metadata.

## What you need

1. A Mac with Xcode 15+.
2. An [Apple Developer Program](https://developer.apple.com/programs/)
   membership ($99/year).
3. `brew install xcodegen fastlane` (fastlane optional but recommended).
4. `rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios`

## Steps on the Mac

```sh
./scripts/build-ios.sh          # Rust libs + lipo + xcodegen
open ios/Topple.xcodeproj
```

1. In Xcode → target *Topple* → Signing & Capabilities: select your team
   (or set `DEVELOPMENT_TEAM` in `ios/project.yml` and regenerate). Change
   the bundle ID from `dev.kasbuunk.topple` if you prefer another.
2. Run on the simulator: everything except Game Center works immediately
   (Game Center needs the app record, next step).
3. In [App Store Connect](https://appstoreconnect.apple.com):
   - *Apps → “+” → New App*: name **Topple — a game of proof**, bundle ID
     as above, SKU e.g. `topple-001`.
   - *App → Services → Game Center*: enable, no leaderboards needed.
   - Age rating questionnaire: everything “No” → 4+.
   - Privacy: “Data not collected” (matches `PrivacyInfo.xcprivacy`).
4. Screenshots: run the app on an iPhone 16 Pro Max simulator and an iPad
   Pro 13" simulator, take landscape screenshots of the title, a mid-cascade
   board, pick-a-side, and the proof view (the same four shots as
   `docs/*.png`). Drop them into App Store Connect or
   `ios/fastlane/screenshots/`.
5. Ship:
   ```sh
   cd ios
   fastlane beta      # TestFlight first — test an online duel between two
                      # devices/accounts before review
   fastlane release   # builds, uploads, submits for review
   ```
   Or without fastlane: Xcode → Product → Archive → Distribute App.

## Review notes worth adding

- “Online duel requires two Game Center accounts; a demo match can be
  played against the reviewer’s second device. All other modes are fully
  offline.”
- Game Center turn-based matches carry ~40 bytes of game state per turn;
  there is no third-party server.

## Gotchas

- Build the Rust libraries **before** the Xcode build; the project links
  `libtopple_ios.a` straight out of `target/`.
- If you rename the bundle ID, keep the Game Center entitlement enabled in
  the App ID (Certificates, Identifiers & Profiles → Identifiers).
- The app is landscape-only by design (the board is 4:3 with pad buttons in
  the pillars); App Review is fine with that as long as Info.plist and
  behavior agree, which they do.
