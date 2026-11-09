# P0' Sidecar Spike Report — 2026-11-09

## Verdict: **PASS** — 官方 bundles 可被外来宿主完整引导，M1 里程碑在 spike 层面已预演通过

### 证明了什么

1. **官方引导机制是公开 API**：`@deepseek-ai/dsh-app-boot` 导出 `boot/loadProfile/createRuntimeResolution/readProfilePatches/PluginPackages`——官方启动器（apps/cli）只是它的薄壳（argv 解析 → runProfile）。sidecar 无需逆向任何东西，直接调用官方函数。
2. **宿主契约极小**（外来宿主只需提供 4 样东西，约 60 行 bootstrap）：
   - `profileContext`（name/dir/patchPath/installAnchor/startedBundles/cwd/home/overlays）
   - `DSH_LAUNCH_ENVIRONMENT_KEY` ← `createLaunchEnvironmentSnapshot([{source:"process", values:env}])`（分层环境快照，非裸 process.env）
   - `PluginPackages` 插件 + `{resolution}` ← `createRuntimeResolution({installAnchor, profile})`
   - `provideCmdline({args:[task], exit, ready})`
   - 然后 `boot("dsh", <profile>/cordis.yml, readProfilePatches(...))` → 完整插件树挂载
3. **端到端两次验证**：
   - 琐碎任务："SPIKE-OK"（真实 DeepSeek 调用，正常退出）
   - **M1 预览**：DSH_HOME 隔离家目录挂 dsh-trustquant skill symlink → agent 定价（NPV 3.9340730674 ✓）、单位口径正确（Vega per 1.00 vol 显式 ÷100）、calculation_id/manifest_hash、**自主 replay → REPLAY: PASS**、自主处理沙箱写权限（TRUSTQUANT_DATA_DIR）

### 关键工程发现（进入 P1' 的输入）

| 发现 | 影响 |
|---|---|
| **最新版 bundles 的传递依赖部分未发布**（dsh-bash-env/dsh-skill-local 等 404） | sidecar 必须钉"已发布版本集"；latest 不可用 |
| 顶层包的依赖声明 ≠ 闭包（dsh-agent 依赖的 dsh-scope 不在任何顶层 deps 里） | 安装策略 = 以官方 vendored 树为基准做闭包（306 包精确版本清单），或直接 `npm i @deepseek-ai/dsh@X` 用其自身树做 anchor |
| cordis 插件家族是独立版本线（1.0.x），与 harness 版本线（0.2.1-rc.5）不同 | 钉版清单要按官方树实际版本取 |
| 腾讯镜像有已发布版本集；npmjs 直连慢且间歇 | 镜像优先 + 钉版本 |
| ERESOLVE 冲突（rc 包互钉） | `--legacy-peer-deps` |
| 默认 profile 含 workspace-write 沙箱 | 插件若写默认路径需 env 覆盖模式（TRUSTQUANT_DATA_DIR 模式）|

### JsHost 契约雏形（P1' Rust 侧）

```
trait JsHost {
    fn spawn(&self, spec: SidecarSpec) -> Result<SidecarHandle>;  // node boot.mjs 等价物
    fn call(&self, op: HostOp) -> Result<...>;                    // IPC: 任务提交/事件流/取消/退出码
}
// sidecar 侧 = 本 spike 的 boot.mjs 产品化：解析 IPC 指令 → 组装宿主契约 → boot()
// Rust 内核经此通道获得完整官方生态；无 sidecar 时走原生 skills/MCP 子集
```

### 下一步（P1'）
- Rust workspace 骨架（JsHost trait + sidecar impl + IPC 协议 v0）
- M1 正式版：Rust 二进制 spawn sidecar 跑通同一条 trustquant 冒烟
- 命名待定（候选：TrustHarness，与 TrustQuant 品牌一致）

## 复现
```bash
cd /home/ubuntu/dshx-spike
nvm use 22; eval "$(grep -m1 '^export DEEPSEEK_API_KEY=' ~/.bashrc)"
DSH_HOME=$PWD/home node boot.mjs "task text"
```
（依赖树按官方 0.2.1-rc.5 vendored 闭包装齐，见 /tmp/spike-deps.json 与 npm3 日志；profile 在 home/profiles/headless）
