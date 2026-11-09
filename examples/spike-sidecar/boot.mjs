// dshx sidecar spike — boot the OFFICIAL dsh-base + dsh-headless bundles
// under a foreign (our own) bootstrap, mirroring the official runProfile
// assembly from @deepseek-ai/dsh's lib/profile-boot (0.2.1-rc.5).
//
// usage: node boot.mjs "one-shot task text"
import { writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";

const task = process.argv[2];
if (!task) {
  console.error("usage: node boot.mjs \"task text\"");
  process.exit(2);
}

// Resolve every @deepseek-ai package exactly the way the official install
// does: through the dsh package's own dependency tree in this workspace.
const root = resolve(fileURLToPath(import.meta.url), "..");
const require = createRequire(join(root, "node_modules/@deepseek-ai/dsh/package.json"));
const importPkg = (name) => import(require.resolve(name));

const { boot, loadProfile, createRuntimeResolution, readProfilePatches, PluginPackages } =
  await importPkg("@deepseek-ai/dsh-app-boot");
const { provideCmdline } = await importPkg("@deepseek-ai/dsh-cmdline");
const { DSH_LAUNCH_ENVIRONMENT_KEY, createLaunchEnvironmentSnapshot } =
  await importPkg("@deepseek-ai/dsh-launch-environment");

const binName = "dsh";
const profileName = process.env.SPIKE_PROFILE ?? "headless";
const home = process.env.DSH_HOME;
const installAnchor = join(root, "node_modules/@deepseek-ai/dsh/package.json");

// 1) load the profile (bundles declared in its package.json)
const profile = loadProfile(binName, profileName, installAnchor, home);
// the root config is always rewritten to the empty entry list (official behavior)
writeFileSync(join(profile.dir, "cordis.yml"), "# spike root — empty entry list\n[]\n");

// 2) plugin-package resolution anchored at the vendored official tree
const resolution = await createRuntimeResolution({ installAnchor, profile });

// 3) the context the official launcher provides (shape copied from runProfile)
const environment = createLaunchEnvironmentSnapshot([
  { source: "process", values: process.env },
]);
const profileContext = {
  name: profileName,
  dir: profile.dir,
  patchPath: profile.patchPath,
  installAnchor,
  startedBundles: profile.layers.map((layer) => layer.packageName),
  cwd: process.cwd(),
  home,
  overlays: [],
  telemetryDisabledEnv: process.env.DSH_TELEMETRY_DISABLED,
};

// 4) boot the composed patch tree; headless reads the task from cmdline args
const ctx = await boot(
  binName,
  join(profile.dir, "cordis.yml"),
  readProfilePatches(binName, profileContext, profile),
  async (hostCtx) => {
    hostCtx.provide("profileContext", profileContext);
    hostCtx.provide(DSH_LAUNCH_ENVIRONMENT_KEY, environment);
    await hostCtx.plugin(PluginPackages, { resolution });
    provideCmdline(hostCtx, {
      args: [task],
      exit: (code) => process.exit(code),
      ready: { onReady(listener) { listener(); return () => {}; } },
    });
  },
);

console.error(`[spike] booted: fiber.state=${ctx.fiber.state} loader=${ctx.get("loader") !== undefined}`);
