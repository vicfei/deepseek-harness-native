# RELEASE-LINEAGE — deepseek-harness-native (`dshn`)

> 每次发版必须更新此表（铁律）。上游基线 / 兼容面 / tag / commit / 内容。

| Version | Tag | Commit | Upstream baseline | Contents |
|---|---|---|---|---|
| **v0.1.0** | `v0.1.0` | (this release) | DSH **0.2.1-rc.5** concept set (frozen); official bundles @ 0.2.1-rc.5 pinned closure (306 pkgs) | First public release. **Pure-Rust native kernel**: session v4 (oracle-pinned to real upstream artifacts), SSE decoder (boundary-independence proven), agent loop (fixture-pinned event sequences), DeepSeek provider (HTTPS streaming), tools + multi-step turn (live-verified tool round-trips), skills (SKILL.md convention + catalog seam), native MCP client (stdio, `mcp__<server>__<tool>` naming contract), approval/policy layer (3 presets, fail-closed gate, session policy events), runtime-context injection (fixture-pinned 3-section shape). **Paradigm kernel** (`dshn-paradigm`): the context paradigm (arXiv 2608.25512) natively — Cx tree, revertible effects (LIFO inverses), reactive coeffects, declarative reconcile; the paper's two theorems as CI property tests. **Sidecar lane**: official dsh-base+dsh-headless bundles boot under a 60-line foreign host (P0 spike PASS); `dshn provision` + `dshn "task"` one-liner. **Live milestones**: M1 (trivial native E2E), M2 (Vibe-MCP + trustquant in ONE native session), M3 (full trustquant smoke pure-Rust). 31 tests green. Apache-2.0. |

## Upstream tracking policy

- Frozen at the **published release set** 0.2.1-rc.5 (sidecar closure: 306 packages, exact-pinned).
- Latest bundles may reference unpublished packages (verified 2026-11-09); the closure pins to a
  published snapshot. Upstream concept evolution adopted on OUR release cadence through the SPEC
  ledger — never chased commit-by-commit.
- The oracle fixtures (2 real sessions + 1 real SSE stream) are captured from this pinned version.

## Architecture summary at v0.1.0

```
dshn (bin)         ── CLI: run / provision / --native / --sidecar / --approval
├─ dshn-core       ── native kernel: session/sse/loop/provider/tools/skills/mcp/policy/runtime
├─ dshn-paradigm   ── the context paradigm (paper's spirit): Ctx/effects/coeffects/reconcile
└─ sidecar lane    ── official bundles via Node ≥22 (pinned closure, boot.mjs host)
```
