# TUTORIAL — compile a program you can watch

*Status: **design-intent, all of it.** The v0 textual IR below is
written against the keel ([README](../README.md)) and the arch lane's
direction (pass = pure function `fabric → (fabric, diff)`; φ as
wire-join) — neither has code behind it yet. The experiments lane
will make this real; when it does, this page gets pinned to a
committed `.expected` file and every transcript below becomes
something you can `diff` against, the way quilt-verilog's
[TUTORIALS](https://github.com/SuperInstance/quilt-verilog/blob/master/docs/TUTORIALS.md)
work. Until then, treat every fenced block as a sketch of a planned
dump format, not output of anything.*

The tutorial's promise is its title: after a compile, nothing is
hidden. Every step is a diff you could have watched live. We compile
one small function through three ticks — parse, constant fold, DCE —
and look at the fabric after each.

## 0. The program

```
func @answer() -> i32 {
entry:
  %six   = const i32 6
  %seven = const i32 7
  %prod  = mul i32 %six, %seven
  %spare = const i32 99          ;; never used by anyone
  ret i32 %prod
}
```

Five values. Four of them matter; `%spare` is here so the dead-code
tick has something honest to do. Syntax is deliberately
LLVM-shaped — the betting is on what's *underneath* the syntax, not
new spelling.

## 1. Tick 0 — parse: program becomes a fabric

The parser produces cells and wires. One **region** for the block;
one **cell** per instruction; one **wire** per operand use. The dump
format we intend (placeholder command: `quilt dump @answer`):

```
fabric f0 · tick 0 · @answer
  region r1 "entry" (func @answer)
  cell  c1  const i32 6        → fanout w1
  cell  c2  const i32 7        → fanout w2
  cell  c3  mul  i32           → fanout w3
  cell  c4  const i32 99       → fanout —
  cell  c5  ret  i32           → fanout —
  wire  w1  c1 → c3.arg0
  wire  w2  c2 → c3.arg1
  wire  w3  c3 → c5.arg0
ledger @ tick 0: admitted c1 c2 c3 c4 c5 · delivered — · dropped — · balanced
```

Read it as: `%prod` is cell `c3`, its two uses of `%six`/`%seven` are
wires `w1`/`w2`, and the `ret`'s use of `%prod` is wire `w3`. `%spare`
(c4) has **no fanout** — nothing wires from it. That fact is already
visible at tick 0, before any pass runs.

## 2. Tick 1 — constant fold: a pass appends a diff

The fold pass (design: pure function over the fabric) finds `c3 =
mul(const, const)`. It does **not** edit `c3`. It appends diff `d1`:

```
diff d1 · tick 1 · pass fold
  + cell c6  const i32 42
  + wire  w4  c6 → c5.arg0
  − wire  w1  detached          (c1 no longer feeds c3)
  − wire  w2  detached          (c2 no longer feeds c3)
  − wire  w3  detached          (c3 no longer feeds c5)
  ~ c3 retired — consumed into c6
ledger @ tick 1: c1 delivered→c6 · c2 delivered→c6 · c3 delivered→c6 ·
                 c4 untouched · c5 untouched · balanced
```

This single block is the whole pass semantics in miniature:

- the new constant `42` is a **new cell** (`c6`), not a mutation of
  `c3` — N4: history appends, never rewrites;
- `c5`'s operand change is two events — wire `w3` detached, wire `w4`
  attached — both recorded;
- the **ledger line** is the conservation law doing its job: c1, c2,
  c3 were admitted to this pass and each is accounted
  (`delivered→c6`). Nothing vanished. If `d1` had detached `w1`
  without a ledger entry for `c1`, the verifier would fail the tick.

The fabric *at tick 1* is the replay of `d1` over `f0`. Watching it
that way:

```
fabric f1 = f0 ⊕ d1 · tick 1 · @answer
  region r1 "entry"
  cell  c6  const i32 42       → fanout w4
  cell  c4  const i32 99       → fanout —
  cell  c5  ret  i32           → fanout —
  wire  w4  c6 → c5.arg0
  (c1 c2 c3 retired — present in history, not in the live fabric)
```

Retired cells are not deleted. They are one `replay 0` away.

## 3. Tick 2 — DCE: deletion with paperwork

Dead-code elimination looks at fanout and the ledger. `c4` has no
wires and never did. Its diff:

```
diff d2 · tick 2 · pass dce
  − cell c4 dropped
  ledger: dropped-unused — no reader at any tick; admitted @ tick 0,
          never delivered, never consumed. Reason recorded.
ledger @ tick 2: all values admitted through tick 2 are delivered or
                 dropped-with-entry · balanced
```

`%spare` did not evaporate — it is **dropped with a ledger entry
naming the reason**. That is D4 made visible: DCE is a pass that
files paperwork. The same query that found `c4` is the red test for
the pass (see §5).

## 4. Output, and the two directions you can walk

**Forward, to the answer.** `c5 (ret)` reads its operand wire `w4` →
`c6 = const i32 42`. The function returns 42.

**Backward, to the origins — a provenance walk.** Ask `c6` where it
came from (placeholder: `quilt why c6`):

```
c6 const i32 42
  ← minted by diff d1 (tick 1, pass fold), which consumed:
     c3 mul i32  [retired]
       ← operands via w1, w2 (detached @ d1):
          c1 const i32 6   [parsed @ tick 0]
          c2 const i32 7   [parsed @ tick 0]
```

That walk — value → diff that minted it → cells that diff consumed →
back to the parse — is the entire intended payoff of
[THEORY §7](THEORY.md): "where did this 42 come from" costs a query,
not a rebuild under a debugger.

**Sideways, to any earlier moment.** `replay 1` gives you `f1` with
`%spare` still alive; `replay 0` the pristine parse. Bisection of a
miscompile is walking ticks, per N4 (D5's replay checks live here).

## 5. What the tests look like before this is real

Following [DOCTRINE](DOCTRINE.md), when the experiments lane lands:

- **D1 red/green:** fold's suite asserts the fabric after fold has
  `c6` wired into `ret` *and* that the pre-fold fabric fails the same
  assertion. DCE's suite asserts `c4` is ledger-dropped *and* that
  without DCE the ledger shows it undelivered. Red without the pass,
  green with it.
- **D4 ledger:** every suite's final tick must reconcile — admitted =
  delivered + dropped-with-entry.
- **D5 replay:** each suite replays tick 0..k and requires
  bit-identical fabrics at every intermediate tick.
- **D3 numbers:** when timings exist, they come with the command that
  produced them. Until then: unmeasured, labeled unmeasured.

## 6. Where this page is lying to you

Labeled honestly, per D8: the dump format is sketched, not
implemented; `quilt dump` / `quilt why` are placeholder names; the
exact diff grammar (`+ − ~`) is design-intent pending the arch lane's
spec and the experiments lane's first real output. Everything here is
written to be *checkable against* whatever lands — the day the
experiments lane produces its first real transcript, this tutorial
either matches it or gets corrected in a commit that starts with
`TUTORIAL:`.

---

*Next: [GLOSSARY](GLOSSARY.md) fixes the words used above. The laws
binding this page: [DOCTRINE](DOCTRINE.md). The argument underneath:
[THEORY](THEORY.md).*
