⊤OPP⊥E
Pronounced "Topple." The name is spelled with the game's own pieces: the T is ⊤, the L is ⊥.

The thesis in one line: two players adversarially build a valuation, one atom per turn, and the only physics the board obeys are the boolean laws. Every move is a choice of branch in the formula's Shannon expansion, every board update is an equational rewrite, and a finished game is a proof. There is no question layer on top — the semantics is the mechanic.

The rules — the actual screen
This is the complete rules card, verbatim, 16 lines × ~40 characters, which renders comfortably on 640×480 at a 24px mono font:

⊤OPP⊥E ─────────────────────────────
One formula. ⊤ wants it to end ⊤;
⊥ wants it to end ⊥.

SETUP  Formula appears. One player
picks a side; the other picks who
moves first.

TURN   Pick any atom on the board and
set it to ⊤ or ⊥. Every occurrence is
replaced at once. You must move.

LAWS   The board then rewrites itself,
one law at a time (mirrors apply too):
 ⊤∧x=x   ⊥∧x=⊥   ⊤∨x=⊤   ⊥∨x=x
 ⊤⇒x=x   ⊥⇒x=⊤   x⇒⊤=⊤   x⇒⊥=¬x
 (⊤=x)=x (⊥=x)=¬x ¬⊤=⊥ ¬⊥=⊤ ¬¬x=x

WIN    The instant a lone ⊤ or ⊥
remains, that side wins. No draws.
Win condition, stated plainly: the board always collapses to exactly one constant — possibly long before all atoms are assigned — and whoever's constant is left standing wins. Atoms that get simplified off the board are gone forever; you cannot assign what no longer exists.

The law table doubles as the entire tutorial: after each assignment, the board animates the cascade one law at a time with a small toast naming the equation that fired ("⊥ ∨ x = x"). A player who has never seen logic learns the laws the way a Go novice learns capture — by watching the physics, not by reading.

Controls (Miyoo Mini Plus). D-pad moves the cursor between atoms; all occurrences of the hovered atom glow together, which silently teaches that assignment is global. Then the layout hands us a perfect mnemonic: X, the top face button, sets ⊤; B, the bottom face button, sets ⊥. A zooms into a subformula on big boards; Y ghost-previews the rewrite cascade of the hovered assignment (a reading aid, disabled in Strict and puzzle modes); Start recalls the rules card. Legibility budget: at most 8 atoms and ~30 glyphs per formula, rendered ≥32px in two lines — every symbol readable at arm's length.

One worked turn
Round position, three atoms, you hold ⊤, the built-in Adversary holds ⊥, you to move (so you get assignments 1 and 3 — parity matters):

(p ⇒ q) ∧ (p ∨ r) ∧ (r ⇒ q)
Reading the board: q is pure — both occurrences sit right of a ⇒, positive positions. p and r are mixed — each appears once in an antecedent (left of ⇒, where ⊤ works against you) and once in the disjunction (where ⊤ works for you). Mixed atoms are poisoned: touching one, with either value, hands ⊥ a one-move kill.

The tempting blunder, p ≔ ⊤, "to lock in the middle clause":

(⊤ ⇒ q) ∧ (⊤ ∨ r) ∧ (r ⇒ q)
  ⊤⇒x = x  →  q ∧ (⊤ ∨ r) ∧ (r ⇒ q)
  ⊤∨x = ⊤  →  q ∧ ⊤ ∧ (r ⇒ q)
  x∧⊤ = x  →  q ∧ (r ⇒ q)
⊥ answers q ≔ ⊥, the cascade runs ⊥ ∧ (r ⇒ ⊥) → ⊥, and the game ends with r deleted from the board unassigned. Loss.

The only winning move is q ≔ ⊤ — the pure atom:

(p ⇒ ⊤) ∧ (p ∨ r) ∧ (r ⇒ ⊤)
  x⇒⊤ = ⊤  (twice)  →  ⊤ ∧ (p ∨ r) ∧ ⊤
  ⊤∧x = x, x∧⊤ = x  →  p ∨ r
One assignment defused both implications and left a fork: two pure atoms under a ∨, with ⊥ to move but you moving last. ⊥ can delete one prong (p ≔ ⊥ collapses it to r) but you take the other (r ≔ ⊤ → ⊤). Any ⊤-assignment by ⊥ is instant suicide via ⊤∨x = ⊤. Forced win in two — the game's "mate in 2" — and I've checked all six alternatives at move one: every other assignment loses. The lesson is genuine tactics and genuine logic in the same breath: polarity of ⇒, constant laws, and a double threat the opponent can't cover.

Solo play and session shape
Fully offline, deterministic from a seed. Four modes: Duel (pass-and-play with the pie-rule setup — one player picks a side after seeing the formula, the other picks who moves first, so lopsided formulas are self-balancing); Adversary (the built-in opponent plays perfectly at board sizes this small; difficulty scales by atom count and operator mix, not by artificial blunders); Puzzles ("⊤ in 2", "⊥ in 3" — forced-win problems, exactly tsumego); and the Daily Gauntlet, five seeded formulas shareable by seed code. The generator solves every candidate formula before serving it and keeps only tense ones — positions where the outcome flips on best play — so you never get dead boards. A round runs 1–4 minutes; a match to three wins lands at 10–18. Squarely inside your 5–20 minute window.

Where the depth comes from
It's provably bottomless. ⊤OPP⊥E is the free-choice Boolean formula game: players alternately choose a variable and its value, one side maximizing truth. The move-order-fixed version of this is exactly TQBF, the canonical PSPACE-complete problem, and the free-choice variants were shown PSPACE-complete in Schaefer's 1978 work on two-person formula games — same complexity class as generalized Hex. (Flag: I'm citing Schaefer from memory; TQBF's status is certain.) Practically: no clean heuristic will ever tame the game family, so skill compounds indefinitely. Honest caveat in the other direction: at the Miyoo's 8-atom cap, any single round is machine-solvable — like a Go endgame or a chess study — and the inexhaustibility lives in the generator and the scaling dial, exactly as chess's finiteness never made it shallow.

Opening theory is DPLL. Strong play means rediscovering solver heuristics as strategy: pure atoms are safe and mixed atoms are tempo liabilities (the worked example above); a subformula one assignment from collapse is a unit threat you must count; and ⇒ inverts polarity on its left, so ⊤'s best move is sometimes x ≔ ⊥ — assigning your opponent's constant to fire ⊥⇒x = ⊤ is the game's sacrifice.

Material and parity. Every simplification can delete unassigned atoms, so you can burn the opponent's resources to steal the last move — a capture that flips parity. And = is the spice operator: a lone (p = q) is mutual zugzwang, because whoever touches it first (say p ≔ ⊤, collapsing it to q) hands the opponent the deciding assignment. Since passing is illegal, endgames become fights over who gets forced to touch the = cell first. That's ko-adjacent tension emerging from one connective.

Evaluation is a skill with a scoreboard. The pie rule means that every round opens with a naked judgment call — read the fresh formula, price it, pick a side or a tempo. Misjudge and you've lost before moving.

The post-mortem is a proof. A finished round's move list, with its cascades, is verbatim an equational derivation; reviewing a loss means reading the refutation line. Studying the game and doing propositional logic aren't analogous activities — they are the same activity. That's the answer to "quiz wearing a game's skin": you are never asked whether something holds; you enact valuations, and the game's lineage is Hintikka's game-theoretic semantics with the fixed valuation replaced by an adversarially constructed one.

Notation flags
Confident: all seven glyphs are Hehner's (a Practical Theory of Programming), including = for boolean equality rather than ≡ or ↔, and he reads ⊤/⊥ as "top"/"bottom" — the side names come straight from him. His treatment of ⇒ as an ordering is also why the polarity strategy feels native to the notation.
Flagged and sidestepped: Hehner has a specific precedence table and "continuing" (chained) uses of = and ⇒ at a second precedence level. I don't trust myself to reproduce them exactly, so the board fully parenthesizes everything instead — a legibility win on a 3.5" screen anyway.
Flagged omissions: Hehner also uses ⇐ and an if-fi construct; both are outside your symbol list and add nothing here (x ⇐ y is y ⇒ x, and ¬(x = y) covers exclusive-or if a variant ever wants it). The rewrite laws on the card are standard identities that appear among his base laws, but I haven't attributed his names for them, since I can't verify those from memory.
One last symmetry worth savoring: the game's title screen needs no logo, because the name already is one — ⊤OPP⊥E, the two win conditions holding up the word between them.

