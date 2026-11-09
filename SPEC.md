# SPEC — scope contract & intentional differences

> Status: v0.2 (P2 opening). This file is the anti-treadmill device: what we build, what
> we deliberately don't, and every conscious divergence from upstream — recorded like the
> compatibility ledger of the earlier `deepseek-harness-rs` attempt.
>
> **Prime directive**: this project is the Rust edition of DeepSeek Harness. The first
> priority is a faithful implementation of its **spirit** (the cordis context paradigm,
> arXiv 2608.25512) and its **kernel** (session, agent loop, provider, tools, skills,
> MCP, approval, profile composition) — behavior pinned to upstream releases by oracle
> fixtures. **Own extensions (auditable sessions, dual chains, …) are allowed only after
> the spirit and kernel are complete, and only as additive components that never diverge
> the core.** The posture of the ledger below is "compatible by default; a difference
> must be ledgered to exist".

## Frozen compatibility baseline

- **Concept set of DSH 0.2.x** (profiles / bundles / skills / MCP tool naming
  `mcp__server__tool` / approval modes / headless one-shot).
- Sidecar runs **published release sets only** (latest bundles may reference unpublished
  packages — verified 2026-11-09; the pinned closure lives in
  `examples/spike-sidecar/package.json`).
- Upstream concept evolution is adopted on OUR release cadence, reviewed through the
  ledger below — never chased commit-by-commit.

## IN (kernel, pure Rust, engine-free)

- headless one-shot agent CLI (`dshn "task"`)
- Node sidecar host: spawn/IPC v0 (task submit, event stream, cancel, exit) via `JsHost`
- native MCP client (stdio first) + skills discovery (the cross-harness SKILL.md convention)
- context-paradigm runtime for native components: Ctx scope tree, effect registry with
  runtime-held inverses (dispose semantics), reactive service activation (coeffects),
  declarative loader with config reconciliation

## POST-PARITY EXTENSIONS (own designs — not before kernel parity)

- auditable sessions: append-only hash-chained event log, supersede-on-record, session
  manifest, digest replay (`dshn replay <session>`)
- dual-chain demos pairing harness sessions with external ledgers
- These ship as additive paradigm components; the kernel stays upstream-faithful.

## OUT (deliberately, for now)

- Web UI, TUI — the headless surface first
- cordis/npm plugin execution inside the Rust process — that is the sidecar's job;
  native extensibility is the NDJSON subprocess tool protocol (inherited concept from
  dsh-rs) + MCP
- V8 embedding (deno_runtime) — **demoted to a future optional optimization**; the
  sidecar lane already delivers ecosystem compatibility without it
- multi-provider LLM — DeepSeek first, provider trait from day one

## Intentional differences ledger (seed rows)

| # | Upstream (official harness) | `dshn` | Why |
|---|---|---|---|
| 1 | plugin runtime intercepts V8 internals (`node-addon-require-builtin`) for ESM loader interposition | ecosystem stays inside the Node sidecar; kernel is engine-free native | V8-bound internals are unportable and binary-hostile; the kernel must stay category-① (engine-free) |
| 2 | sessions are cordis-mediated, no replay receipt | append-only + supersede-on-record + digest replay | auditability is this project's thesis |
| 3 | everything-is-a-plugin inside one process | paradigm-native kernel + protocol boundary (sidecar IPC, MCP, NDJSON tools) | single musl binary; local buildability on small machines; the trilemma resolution |

## Oracle methodology

The Node sidecar doubles as the **oracle fixture generator**: for any input, run the
official sidecar, record its event stream (and SSE stream via a recording proxy) as
fixtures; the native implementation must reproduce them (normalized) before its slice
is called done. This is the `deepseek-harness-rs` methodology, minus the treadmill:
we pin one published release set and move it on our cadence.

## Non-goals & honesty rules

- Never present synthetic/sidecar-derived numbers as kernel performance.
- The sidecar requires Node ≥22 (official ecosystem constraint). `dshn` without a sidecar
  must still run skills/MCP natively.
- Trademark posture: community namespace, explicit non-affiliation (README first line),
  no upstream branding assets.
