# SCOUT-REPORT — fleet + LLVM/MLIR survey + ideation for quilt-llvm v0

**Date:** 2026-08-30 · **Lane:** QUILT-LLVM scout + ideation · **Scope:** Part A fleet reuse, Part B LLVM/MLIR doctrine (with prior-art honesty), Part C multi-model ideation → recommended v0 shape.

**Method note (house law).** Everything below was read from the actual repos (shallow clones under `/tmp/scout`, since cleaned) or fetched from live docs on the date above. Model passes carry attribution per house law; lane failures are booked, not hidden. Where a claim is my judgment rather than a measured fact, it says so.

---

## Part A — Fleet scout: what exists, what ports, what warns

Eleven repos studied (`gh repo view` + shallow clone): quilt-verilog, quilt-scratch, quilt, quilt-rust, zeroclaw-dissertation, edge-compiler, ternary-spiral, tit_quilt_elixir, quilt-opt, quilt-linker, quilt-conformance. Fleet-wide context: SuperInstance main repo (500+ repos, onboarding doctrine).

### Reuse table

| Repo | What exists | What ports to quilt-llvm | Warning carried |
|---|---|---|---|
| **quilt-verilog** | 5+1 opcodes in pure Verilog-2005; 18-bench testbench suite; 6 SymbiYosys formal proofs (5 BMC + 1 k-induction; conservation family T1/SER/DROP/FAN BMC-55-proved); QUF binary container spec (the "GGUF of cellular silicon"); `make test/sim/formal/synth/pnr` all reachable; CI verify-all; ACADEMIC-RIGOR.md with per-invariant proof status | **(1) The verification culture:** every README number re-run with a date, every claim reachable from a make target. **(2) The conservation invariant family** — T1 (transport: emit = pipe + accepted), SER (commits serialize), DROP (booked ≤ N cycles, no silent drop), FAN (every emission well-formed) — these become the verifier's checks, promoted from fabric to IR. **(3) QUF's lesson:** a flat, self-describing container with unknown-section skip (never fatal) is the model for IR serialization (`.quiltf` fabric files). **(4) The +1 ack/nak law:** every op is answered, nothing left hanging → every verifier query returns a verdict, never a shrug. | Conservation is a **design target, not a proven invariant**: formal conservation FAILS in prove mode at L1/L2; BMC-55 only, k-induction pending. Carry the asterisk verbatim — do not launder "conserved" into a theorem. The silicon identical-hash incident (0eb231b, adjudicated 2026-08-30: the F2 ringport flit-cloning bug corrupted every downstream readout; fixed f7027c4) teaches the phantom-hash law: an identical hash can mean a broken sensor, not deep identity. |
| **quilt-scratch** | TILE-CONTRACT.md — normative M1–M4 / N1–N4 laws (inspectable, swappable, fail-visible, saveable; never reach into another tile except through a wire; never destroy history — rewires append); fabric JSON capture incl. rewire history; starting-state rule; IDEATION-2026-08-30.md with an adversarial kid-user attack pass | **The contract-law template, verbatim-adapted:** a cell that breaks a MUST is a bug; breaks a NEVER is a recall. M2 (swappable, wires kept by port name) is precisely pass-replacement semantics. N4 is already keel law. The **fabric file** (M4: full state + history in one JSON) is the save format. | The sharpest attack finding ports directly: **silent no-ops** — "a legal wire the receiver ignores" is the worst failure mode (M3 violated in spirit). IR law: every input port must be load-bearing; a wire the consumer ignores is a verifier error, not a shrug. Also: fan-in is a wall — multi-wire input needs an explicit merge cell (phi), never implicit coercion. |
| **quilt (TS) + quilt-rust** | Reactive typed cellular runtime: cells with kinds, `add_dep` wires, reactive engine, 115 tests (TS); statically-linked Rust binary port, MCP-native | The **engine core** (define/wire/set/evaluate) is the interpreter substrate for a fabric — an IR interpreter is a reactive engine with eager, deterministic evaluation. Cell addressing + the event model port. | Reactive *tick* semantics ≠ compile-time semantics. Don't conflate: an IR fabric is evaluated eagerly and deterministically (no sensors, no time); the runtime's eventual-consistency habits would be a bug in a compiler. |
| **zeroclaw-dissertation** | THESIS-V3 ("The Field Is the Manifold"): conserved manifold state; the observable ladder H1→H5 with **explicit measured fibers** per observable; "every practical hash trades fiber for dimension, on purpose"; FABRIC-LITMUS-1 falsification template; failure-as-theorem-in-disguise discipline | **The fiber discipline for fabric hashing:** any structural hash/signature of a fabric must declare which equivalence it observes. Trivial injective hash (H3) exists but is costly and discontinuous; every cheaper hash has a nontrivial fiber — name it or don't ship it. This directly disciplines the diff-integrity check and Merkle hashing in Part C. The **litmus template** (two arms, pass/fail thresholds, obituary clause) is the test-plan format for pass correctness. | The v1/v2 reader lines folded twice on first-moment instruments (53+-dim fibers). Lesson: don't build analyses (liveness, aliasing) on under-observant signatures — a signature that can't distinguish two fabrics is a bug in the signature, and it will be found by a counterexample gadget, not by review. |
| **edge-compiler** | Cloudflare Worker; real, tested FP32→INT8 quantize pass; async job tracking via KV; honest status page for the broken `/api/compile` (depends on a nonexistent `@cf/onnx` model) | The **quantize pass** is the fleet's only existing "real, tested transform-as-a-service" — a pattern reference for pass packaging (endpoint in, artifact out, status queryable). KV job tracking = the compile-job ledger pattern. | The warning is the repo's own honesty: a pipeline stage depending on an assumed external service that doesn't exist. v0 passes must run on local inputs only; no fabric pass may depend on an external model service. |
| **ternary-spiral** | Deterministic RPS cyclic-dominance cellular automaton (Rust), spatial entropy/biodiversity metrics | Minor: deterministic CA harness + entropy metrics could later feed **curriculum-compile visualization** (fabric "heat" over pass sequences). Not v0 material. | Research toy. Do not port physics into IR semantics. |
| **tit_quilt_elixir** | 5 opcodes as BEAM primitives; doctrine-as-runtime-semantics (let it crash = supervision; crash-safe journal = immutable state; witnesses = hash-chained journal + self-auditing); 16/16 tests, warnings-as-errors clean | **The journal/witness pattern** — immutable append-only entries that a raise can never tear — is the implementation reference for the Weft (pass-history ledger). Hash-chained self-audit = diff-integrity check. | Elixir proves the *doctrine* maps to a runtime; it does not prove an IR. Port the journal, not the BEAM alignment claim. |
| **quilt-opt** | **5 algebraic laws as optimizer passes** for cell-graphs (LINK transitivity, LINK idempotence, VIEW purity, +2); 11 tests proving the laws hold | The **law-as-pass pattern**: each optimization is a stated algebraic law with a test suite per law. This is the direct ancestor of quilt-llvm's pass suite — the same shape, lifted from runtime cell-graphs to IR cell-graphs. | Laws are for the runtime opcodes (BIND/LINK/EFFECT/VIEW/TICK), not SSA values. The *pattern* ports; the laws do not. Re-derive laws per cell type; do not import. |
| **quilt-linker** | Linker for `.qm` cell-graph modules: resolves LINKs, **catches dangling links and depends_on cycles at compile time**; 13 tests in 0.3s | The dangling-link and cycle checks are literally a **verifier front-end already written** (for runtime cell-graphs). Port the checks; they become wire-resolution and cycle-sanity checks on fabrics. | Same caveat as quilt-opt: module-level, not instruction-level. |
| **quilt-conformance** | Deterministic 36-program corpus run against **all five** quilt VMs, 5-way diff, bug ledger (11 filed bugs), honest status table (7 MATCH / 29 DIVERGE; 4/5 upstream repos fail to build or test) | **The conformance culture template:** one deterministic corpus, an implementation matrix, a bug ledger, zero "trust me it passes" claims. This is the test-culture backbone for quilt-llvm: the corpus is the authority, the matrix is the honesty. | Its own finding is the warning: five implementations of one spec diverged in 29/36 cases. Spec + prose is a rumor; spec + corpus + ledger is a spec. v0 ships all three. |

### Top reuse finds (ranked)

1. **quilt-verilog's conservation invariant family (T1/SER/DROP/FAN) + its asterisk** — the verifier's check list is half-written, and the honest "BMC-55 only, induction pending" status format prevents the biggest sin (claiming proved what is only checked).
2. **quilt-conformance's corpus/ledger/matrix triad** — the test culture, lifted whole.
3. **quilt-scratch's TILE-CONTRACT M/N law template + the silent-no-op warning** — the cell-type contract and its sharpest known failure mode.
4. **quilt-opt's law-as-pass pattern** — pass suite shape with per-law tests.
5. **tit_quilt_elixir's hash-chained journal** — the Weft's implementation reference.
6. **THESIS-V3's fiber discipline** — fabric hashing must declare its observable, or fail to a counterexample gadget.

---

## Part B — LLVM/MLIR doctrine (web, 2026-08-30)

Sources fetched live: llvm.org GettingStartedTutorials, LLVM TestingGuide, MLIR LangRef, MLIR Rationale index (incl. Generic DAG Rewriter, Linalg, Incremental Adoption). Gemini web_search quota-exhausted (429 ×2, booked) — direct fetches used instead.

### What made LLVM world-class (and what we adopt)

1. **Library-first design.** A compiler is a set of reusable libraries (Programmer's Manual culture) over an explicit IR, not a monolithic driver. → quilt-llvm: passes are swappable libraries over a stable in-memory fabric; the driver is a pass sequence.
2. **One IR, three forms, round-tripped.** MLIR LangRef states it exactly: "human-readable textual form suitable for debugging, an in-memory form suitable for programmatic transformations, and a compact serialized form suitable for storage and transport — all describe the same semantic content." → the fabric gets: textual `.qf` (debuggable), in-memory cells (transforms), serialized fabric+Weft (storage). Round-trip is a conformance test, not an aspiration.
3. **A normative LangRef.** Every instruction's semantics written down before (or alongside) its implementation; the LangRef outranks the code. → one spec page per cell type, 13-ish of them, no more.
4. **Explicit pass management.** Passes declare analysis dependencies and what they preserve; invalidation is explicit. → a quilt pass declares: reads (fabric, analyses), writes (new fabric + diff), preserves (cells untouched, by content hash).
5. **Three-tier test culture** (TestingGuide): unit tests for support libraries; **regression tests** — small distilled IR per bug, driven by a lit-style runner with FileCheck-style matching; **test-suite** whole programs for end-to-end + performance. Analyses tested via **printer passes** that print their findings for checking. → adopted wholesale, plus the fleet's red/green keel rule (a pass's test goes red without the pass).
6. **IR stability + auto-upgrade** (bitcode compatibility culture). → the Weft must survive fabric-format version bumps: unknown sections skip, never fatal (QUF's rule, same as GGUF's).
7. **Onboarding rungs as small as "MyFirstTypoFix."** → the curriculum compile (Part C) is this doctrine turned up: every intermediate state inspectable means every contributor can watch what a pass did.

### What LLVM/MLIR does NOT do — that cells make natural

- **No first-class provenance.** LLVM's "why is this value here" lives in bolt-ons (debug info, remarks, git-bisect-on-IR folklore). MLIR has no persistent, walkable provenance edge per value. Cells make provenance a *wire you walk*, not an archive you grep.
- **No re-runnable history.** Pass managers discard intermediates; `--print-after-all` is a debug dump, not a structure. There is no standard append-only diff log with parent-hash chaining. The Weft is genuinely missing from both.
- **No conservation law.** Silent drops are normal and correct in LLVM (DCE removes without ledger). The ledger-drop discipline ("removed AND recorded") does not exist anywhere in mainstream IRs.
- **No pedagogical stance.** LLVM IR dumps are for experts. A fabric where every intermediate state is a saveable, inspectable, swappable artifact is a *curriculum* for teaching compilation itself — nobody ships that.

### Prior art, honestly

- **SSA** (Cytron et al., 1991) — the foundation; a cell IR that isn't SSA-shaped on the value side is just a worse IR. We keep SSA values; cells add the fabric around them.
- **Sea of nodes** (Click, 1995; Graal/Truffle today) — values + explicit effect edges. The memory-token threading proposed below **is the sea-of-nodes effect list re-derived**; we should say so and cite Click rather than pretend novelty.
- **CIL / Cranelift IR** — pragmatic prior art for compact, verifiable non-LLVM IRs with checkers.
- **MLIR — the sharpest honesty point:** *dialects ≈ cell types is a near-total overlap.* Custom ops with custom types (dialects), port contracts (traits/interfaces), declarative local rewrites (generic DAG rewriter / PDL), round-tripped textual IR (LangRef's three forms), progressive lowering between abstraction levels — MLIR has all of it, battle-tested at Google scale, with a decade of ecosystem. **What MLIR does better than quilt-llvm will ever do soon: everything operational** — codegen, targets, maturity, community, incremental adoption story (its own rationale doc refutes needing full adoption to benefit). The honest residue that is *not* MLIR overlap: **the ledger** (append-only diff history with hash-chained integrity) and **first-class provenance wires + the conservation law**. And even that residue has a closer cousin in…
- **e-graphs / egg** (Willsey et al., 2021) — equality saturation records rewrites in an e-graph and can extract explanations; this is the strongest existing "rewrites with recorded justifications" and should be **studied hardest**. The honest difference: an e-graph is a per-run fixpoint data structure, discarded after extraction; the Weft is a persistent, cross-pass, content-addressed history with conservation obligations. Related-but-different, and we should say which parts are egg's (justified rewrites) and which are ours (persistent ledger + conservation).
- **CRDT-based IR editing** — near-blank prior art (CRDT collaborative editing exists for text/ASTs; nobody ships a CRDT compiler IR). Genuinely open — and honestly, possibly open *for good reason*: a single-writer compiler doesn't need merge semantics. Treat CRDT-IR as a v2+ research question, not a v0 feature. (Mistral's lane argued CRDTs should be studied hardest; my synthesis overrules it for v0 — single-writer history doesn't need merge, and the r1/Sonnet lanes point at egg as the load-bearing prior art. Disagreement recorded, per house law.)

---

## Part C — Ideation passes (multi-model, attributed)

### Booked failures (house law)

| Lane | Failure |
|---|---|
| web_search (Gemini) | 429 quota-exhausted ×2 — fell back to direct doc fetches |
| DeepInfra: Seed-2.0-pro, Qwen3.6-35B-A3B, Hermes-3-405B | **All three blocked**: `inference prohibited, you have reached user-set limit` (user-set billing cap on DeepInfra). Substitutions below; originals re-run when the cap lifts. |
| DeepSeek direct API (V4-Pro/Flash) | `Insufficient Balance` (key valid, models listed, wallet empty) |
| ollama deepseek-r1:8b | Timeout at 240s mid-answer; Q1 critique recovered, Q2+ lost to a mistral:7b continuation lane |
| claude haiku (1st attempt) | Stalled asking a clarifying question instead of delivering — lesson: small models need a deliver-now clause |

### Pass 1 — deep planning · **Claude Sonnet 5** (substituting for Seed-2.0-pro)

Full output archived in the session transcript. Core findings, condensed:

- **Cell types with ports + LLVM analogs** (13–14): `const`, `param`, `binop`, `cmp`, `cast`, `stackslot`, `gep`, `load`, `store`, `call`, `phi`, `br`, `ret`, (optional `select`). Structural cells: **type-cell, block-cell, edge-cell, func-cell**.
- **The load-bearing insight — memory tokens:** model memory as an explicit **linear token wire** threaded through effectful cells (`load: addr, mem_in → out, mem_out`). This converts "the heap" from an implicit global into an explicit wire, making the conservation law checkable for side effects, not just SSA values. (Honesty: this is sea-of-nodes effect threading, re-derived — cite Click.)
- **phi keyed by edge-cells, not predecessor blocks** — control-flow edits (edge splitting) can't desync the merge; `param` as a root cell (no input wire) makes inlining "rewire param wires to caller args" instead of a special case.
- **Pass ranking by cell-feasibility:** DCE (trivial — pure reachability over first-class def-use wires; conservation *forces* a `dropped{cell, reason}` ledger entry per removal) → copy-prop (nearly a no-op: identity cells rewire) → constant folding (local, no fixpoint) → CSE (easier than LLVM: structural hash `(op, type, input-wire-ids)`; redundant-load elimination is the first effect-chain reasoner) → **inlining hardest** (large structural diffs stress the history and namespace merge most).
- **Verifier checks, priority order** (8): wire resolution → conservation ledger balance → effect-token linearity → SSA dominance (phi: producer dominates the *edge*, not the block) → type agreement → control well-formedness → diff/history integrity (apply diff n to parent hash → child hash; no cycles) → orphan sanity (unreachable ⊕ absent-from-ledger = error).
- **Biggest risk of append-only history:** replay cost and log size (200–300 passes at -O2 scale). **Mitigation:** Merkle content-addressed cells (untouched blocks stored once — storage O(actual change)) + periodic materialized checkpoints as *derivable cache, never authority* (load checkpoint, apply ≤K diffs). Bonus: checkpoint hashes give free bisection.
- **Open design call left to the house:** single linear memory-token chain (simple, forces total order) vs per-alias-class tokens (precise load-elim, complex linearity check). Sonnet leans single-chain for v0. Synthesis concurs.

### Pass 2 — logic critique · **deepseek-r1:8b (local)** Q1 + **mistral:7b (local)** Q2–Q4

- **r1 (Q1 — where the model breaks vs SSA):** (a) *dominance violations* — swappable cells + diff passes can produce use-before-def unless dominance is an explicit verifier invariant, not an emergent one; (b) *side-effect ordering* — flexible cell reordering vs SSA's strict dependency order; replaying diffs without order ties can apply effects in the wrong sequence; (c) *interop overhead* — no direct integration with SSA's dominance/ordering properties means interop with LLVM/MLIR analyses requires translation layers.
- **mistral (Q2 — diff-history failure modes):** linear history-size growth (perf/memory); replay divergence (diffs applied out of order → wrong result); diff ≠ semantic equivalence (a syntactically-clean diff can misrepresent what the pass actually did). → the Weft needs: ordered application (parent-hash chaining — already in the design), and *semantic* diff validation (the conservation ledger is exactly this check).
- **mistral (Q3 — prior art):** names e-graphs/egg and CRDTs; argues CRDTs hardest (for distributed diff-history consistency). Synthesis overrules for v0 (single-writer; egg is the load-bearing study) — disagreement recorded above.
- **mistral (Q4 — the one genuine cell win):** "every value's provenance is a walkable wire; each cell inspectable and swappable independently — not natively supported in LLVM/MLIR." (Converges with Part B's honest residue.)

### Pass 3 — naming & lore · **Claude Haiku 5** (substituting for Hermes-3-405B)

| Category | House name | Rationale |
|---|---|---|
| Arithmetic | **Stitch** | weaving values; the basic act of construction |
| Comparison | **Fathom** | measuring the depth between two values |
| Cast | **Splice** | joining rope of different gauges |
| Memory load | **Haul** | pulling cargo up from the hold |
| Memory store | **Stow** | settling values into the hold |
| Call | **Signal** | hailing another function across the wire |
| Branch | **Veer** | a ship's decision point |
| Phi/Join | **Confluence** | where divergent paths merge their waters |
| Constant | **Anchor** | the immovable reference |
| Block/Region | **Hold** | the compartment that structures the cargo |

- **Pass-history ledger: the Weft** — "the horizontal threads appended row by row as the loom runs; the warp (program structure) is fixed; the weft (transformation record) grows. History builds by addition, never retrograde."
- **Verifier: the Tallyman** — "tally in, tally out; no value slips through unaccounted."
- **Curriculum compile metaphor:** *a tapestry suspended mid-weave — every stitch visible, labeled, and traceable to its origin.*

### Synthesis decisions from the three passes

1. **Dual naming discipline** (synthesis rule, satisfying law 4's no-vocabulary-laundering): canonical identifiers are the functional names (`binop`, `load`, `phi`… — searchable, LLVM-mappable); house names (Stitch, Haul, Confluence…) live as the lore/display layer, exactly as QUF names sections functionally and speaks nautically. Never name a capability only in lore.
2. **Memory tokens adopted** (Sonnet), credited to sea-of-nodes (Click) — single linear chain for v0, per-alias-class tokens deferred and noted as the known precision ceiling for load elimination.
3. **Conservation ledger doubles as the semantic-diff check** (r1×mistral convergence): replay divergence and diff≠semantics are both caught by ledger balance — one mechanism, three named failure modes covered.
4. **r1's dominance warning promoted to verifier check #4** with the phi/edge refinement from Sonnet.

---

## Recommended v0 shape (the deliverable)

**Cell types (11 instruction + 5 structural).** Instruction: `const`(Anchor), `param`, `binop`(Stitch), `cmp`(Fathom), `cast`(Splice), `gep`, `load`(Haul), `store`(Stow), `call`(Signal), `phi`(Confluence), `br`+`ret` (Veer family). Structural: `type`, `block`(Hold), `edge`, `func`. Every cell: typed ports, attribute bag, content hash. Memory = explicit linear token wire. `stackslot` and `select` deferred to v0.1.

**Pass list, v1, in build order:** 1. DCE (reachability + forced `dropped` ledger entries) → 2. copy-propagation (identity-cell rewire) → 3. constant folding (local) → 4. CSE (pure cells; structural hash) → 5. simple inlining (hardest, largest diffs — ships last).

**Verifier (the Tallyman), checks in priority order:** wire resolution → conservation ledger balance → effect-token linearity → dominance (phi keyed by edge-cells) → type agreement → control well-formedness → Weft integrity (parent-hash chain; diffs reproduce child hashes) → orphan sanity. Per quilt-verilog's asterisk: every check reports checked-vs-proved status honestly.

**History (the Weft):** append-only diffs, Merkle content-addressed cells (untouched blocks stored once), checkpoint every K passes as derivable cache, never authority. Unknown sections skip, never fatal (QUF/GGUF rule).

**Test culture plan:** the triad — (1) red/green regression tests per pass (lit/FileCheck-shaped; goes red without the pass, green with); (2) deterministic conformance corpus + bug ledger (quilt-conformance pattern) incl. textual↔in-memory↔serialized round-trip; (3) litmus runs for pass correctness (two arms, thresholds, obituary clause — THESIS-V3's template). Every number in every doc reachable from a make target, re-run dates recorded.

**Honesty box (pinned):** dialects ≈ cell types is near-total MLIR overlap; the novel residue is the Weft (persistent justified-rewrite ledger) + conservation + first-class provenance, and egg is the closest prior art for the rewrites half. v0 is a toy by declaration; the docs will say so until the corpus says otherwise.

---

*End of scout report. Failures booked above; no claims laundered. Next lane: architecture doc (cells as IR, the theory), then Cell IR v0 + Tallyman.*
