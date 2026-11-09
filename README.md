# deepseek-harness-native (`dshn`)

> **Independent community project — NOT affiliated with DeepSeek, not an official product.**
> 声明：独立社区开源项目，与 DeepSeek 官方无隶属关系。The name follows the `dsh-*`
> ecosystem namespace convention (same convention as `dsh-quant`, `dsh-trustquant` and the
> other community plugins in the official plugin market), and exists to be findable by
> people searching for a native DeepSeek Harness runtime.

[![CI](../../actions/workflows/ci.yml/badge.svg)](../../actions/workflows/ci.yml)
[![status: P0 spike PASS](https://img.shields.io/badge/P0%20spike-PASS-2e7d32)](docs/P0-spike-report.md)

**A Rust runtime for the DeepSeek Harness ecosystem: a pure-Rust kernel (single binary,
auditable sessions, the context paradigm implemented natively) plus a Node sidecar that
boots the official plugin ecosystem as-is.**

中文一句话：纯 Rust 内核（单二进制、可审计会话、时空可组合范式的原生实现）+ Node
sidecar 原样承载官方插件生态——**运行生态，而不是追赶生态**。

## Why this shape

Three facts, all measured on this repo's own spike (see the [P0 spike report](docs/P0-spike-report.md)):

1. **The official bundles boot under a foreign host.** The official boot machinery is a
   public package (`@deepseek-ai/dsh-app-boot`); a ~60-line bootstrap provided 4 host
   services and ran the full official tree (307 packages) end-to-end, including a
   third-party skill invoking an external oracle with a replay receipt. Compatibility is
   proven, not promised.
2. **The ecosystem is V8-bound at its core.** Its runtime interception reaches V8 C++
   internals (`node-addon-require-builtin`) — Bun (JavaScriptCore) fails at boot with
   *"required V8 current-context symbols were not found"*. So the sidecar lane is
   Node/deno; the kernel itself is engine-free native Rust.
3. **Chasing the upstream surface is a treadmill; running it is not.** The sidecar runs
   the official bundles at a pinned release set, so the ecosystem moves when upstream
   moves — the Rust kernel owns only the stable substrate (the Codex-Rewrite lesson, and
   the exact fate the earlier solo `deepseek-harness-rs` attempt did not survive).

Architectural lineage: the official harness implements the context paradigm
([Spatiotemporal Composability, arXiv 2608.25512](https://arxiv.org/abs/2608.25512)) —
revertible effects, reactive coeffects, one mediated context. **This project's first
priority is that spirit and the harness kernel, faithfully, in Rust** — oracle-pinned
to upstream releases. Own extensions (auditable session chains in the discipline of
[dsh-trustquant's ahif × TrustQuant demo](https://github.com/vicfei/dsh-trustquant))
come after kernel parity, as additive components (see SPEC.md's prime directive).

## Roadmap

| Phase | Deliverable | Hard milestone |
|---|---|---|
| P0 ✅ | sidecar spike | official bundles booted by a foreign 60-line host; trustquant skill + replay PASS ([report](docs/P0-spike-report.md)) |
| P1 | Rust workspace: `JsHost` trait, Node-sidecar IPC v0 (NDJSON event contract), headless CLI, node≥22 fail-fast gate | **M1 ✅ 2026-11-09**: `dshn` spawned the sidecar, ran the official bundles, and the trustquant skill priced the option (NPV/unit-correct) with a self-initiated `REPLAY: PASS` |
| P2 | **context-paradigm kernel** (the paper's spirit, native): Ctx scope tree, typed services, effect registry with runtime-held inverses, reactive coeffect resolution, declarative loader with config reconciliation — the two theorems as property tests | paradigm crate green: temporal (unmount == exact state revert) and spatial (interleaved components undisturbed) composability proven in CI |
| P3 | **native harness kernel** (oracle-pinned, slice by slice): session event-sourced core, DeepSeek provider (SSE), agent loop (turn/step/attempt), tools, skills, MCP client, approval — sidecar doubles as oracle fixture generator | parity milestones: native trivial task == sidecar trace; then the trustquant smoke, then Vibe-MCP + trustquant in one native session |
| P4 | native profile/bundle composition + parity hardening (compat ledger complete for the 0.2.x line) | official profile directory boots natively |
| P5 | musl single-binary release; npm platform packages (`dshn`) | zero-Node install for the kernel; `bun install`-provisioned sidecar |
| P6 | **own extensions, post-parity only** (additive): auditable session chains, dual-chain integrations | — |

## Try the spike (P0, no Rust needed)

```bash
cd examples/spike-sidecar
npm install --legacy-peer-deps   # pinned 0.2.1-rc.5 closure (or: bun install — 7s)
node boot.mjs "say something"    # boots the OFFICIAL dsh-base+dsh-headless tree
```

Details, host contract, and the V8-binding findings: [docs/P0-spike-report.md](docs/P0-spike-report.md).

## License

Apache-2.0. The sidecar lane runs official MIT packages (`@deepseek-ai/*`) as published;
this repository contains no upstream code.
