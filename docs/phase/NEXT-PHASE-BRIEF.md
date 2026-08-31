# NEXT-PHASE BRIEF (foreman handoff to strategic lane, 2026-08-30)

You are the strategic operations pass for the SuperInstance fleet. Think hard, iterate, and produce the NEXT PHASE plan: its rounds of iterative sprints and R&D tracks. Be honest, undersell, failures first-class. Numbers or it didn't happen — but this pass is planning, so label judgment as judgment.

## Where the fleet actually is (all verified by the foreman, hashes reachable)

### quilt-llvm (LLVM-class compiler on quilt principles) — master 0edf7d6
- experiments/llvm-fabric: 99/99 green. v0 (11 cells, verifier V00–V15, constfold+DCE, N4 history) + v1: control wires (provenance crosses control edges; V16 phi-mux, V17 acyclicity), inlining with fold-through-graft, Weft per-tick hash-chained signatures (~1× print overhead), mechanical progress law (10k/10k corpus).
- Fuzz corpus 10k fabrics, 255k cells walked, 0 failures. Honest gaps booked in EXPERIMENTS.md §8.3–8.4: region-edit vocabulary, incremental signatures, maintained use tables; provenance doesn't cross ALL control edge kinds yet.
- Docs: keel README, THEORY, DOCTRINE (8 enforceable laws), TUTORIAL, GLOSSARY, ARCHITECTURE, SCOUT-REPORT, REVERSE-ACTUALIZATION, EXPERIMENTS.
- Key idea bank: IR as inspectable cells; passes as pure fn fabric→(fabric,diff) with ledger; conservation law (values delivered or dropped-with-ledger); DCE-as-decay; differential testing vs gcc; batten-spike result: routing near-equivalent pipelines earned only +7.5pts (26% cost saved, 99% utility) — real use is the verification cascade (cheap-tier vs full-walk) where cost spread is real.
- Open M-ladder: M3 (ledger pass manager), M4 (DCE-as-decay, load-bearing), M5/M6 undefined-ish.

### quilt-scratch (no-code kids' game engine) — master c401b29
- Engine 89/89, ta-bridge 23/23, vibe-panel 23/23. Live at fleet-static-host/quilt/. Sites feature it.
- Amplifier tile shipped (amp-cloud/amp-local): the learning loop is a cell (play→events→lessons→hints), mock-transport tested, works with live bridge https://ta-bridge.casey-digennaro.workers.dev.
- Vibe-panel Worker deploy-ready, NOT deployed (needs Casey's secrets).
- Open: TA bridge prod decisions (cron home, event cadence), hard-refresh P2 fixed; remaining story threads (Mo's Ledger, First Owner artifacts, companion pull-lines); level content; kid testing.
- TILE-CONTRACT M/N laws normative; one-new-tile-per-level rule.

### Fleet context
- SuperInstance org = 45+ repos (quilt family, fleet-*, elephant/JEPA, The Tap MUD, pincher, zero-claw dissertation, ai-writings). Tapestry doctrine: failures first-class. Cross-pollination survey in flight.
- Doctrine anchors: iceberg vision (The Tap ↔ real F/V EILEEN boat, Wesley grows), conservation + progress laws everywhere, N4 append-only, prove-before-claim, archive-by-rename.
- Resource reality: GLM-5.3 unlimited (Z.ai Max), DeepSeek cheap but needs top-up, DeepInfra/MMX quota-limited, Claude Pro (Opus/Sonnet daily use OK, Fable golden-ticket only).

## What the phase plan must decide
1. Phase THEME (one sentence) + why now.
2. 3–5 sprint rounds (each: goal, lanes, exit criteria/measured, duration), ordered by dependency.
3. R&D tracks running alongside (high-variance, e.g. DCE-as-decay, verification cascade with battens, region vocabulary, JEPA-room-sense in scratch, kid testing protocol).
4. Which open Casey decisions block what (vibe-panel deploy, key top-ups, TA prod cadence).
5. Kill criteria per track — what evidence ends it.
6. The one thing most likely to fail and the cheapest experiment to find out early.

Deliver as a written plan (docs/phase/NEXT-PHASE.md). Iterate across rounds with the foreman and other models as sounding boards. Undersell; label speculation.
