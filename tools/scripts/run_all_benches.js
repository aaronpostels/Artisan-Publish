#!/usr/bin/env node

const { spawnSync } = require("child_process");
const fs = require("fs");
const path = require("path");

const ROOT = path.join(__dirname, "../..");
const BEVY = path.join(ROOT, "benches/bevy_bench");
const FLECS = path.join(ROOT, "benches/flecs_bench");
const RESULTS = path.join(ROOT, "results");

const argv = process.argv.slice(2);
const quick = argv.includes("--quick");
const skipBuild = argv.includes("--no-build");
const onlyIdx = argv.indexOf("--only");
const only = onlyIdx >= 0 ? argv[onlyIdx + 1] : null;

const REPS = quick ? "3" : "15";
const WARMUP = quick ? "1" : "3";
const SCALE = quick ? "0.25" : "1";

const t0 = Date.now();
const log = (s) => console.log(s);
const step = (s) => console.log(`\n\x1b[1m▸ ${s}\x1b[0m`);

function run(cmd, args, opts = {}) {

  const needsShell = process.platform === "win32" && ["cargo", "cmake", "npm"].includes(cmd);
  const r = spawnSync(cmd, args, {
    stdio: "inherit",
    shell: needsShell,
    ...opts,
  });
  if (r.status !== 0) {
    console.error(`\n✖ failed: ${cmd} ${args.join(" ")}`);
    return false;
  }
  return true;
}

const failures = [];
function stage(name, fn) {
  step(name);
  try {
    if (fn() === false) failures.push(name);
  } catch (e) {
    console.error(`✖ ${name}: ${e.message}`);
    failures.push(name);
  }
}

fs.mkdirSync(RESULTS, { recursive: true });

if (!skipBuild) {
  stage("Building Artisan runners", () =>
    run("cargo", ["build", "--release", "--bin", "bench_fair", "--bin", "bench_ecs", "--bin", "verify_render_cache"], { cwd: ROOT }),
  );
  stage("Building Bevy runners", () =>
    run("cargo", ["build", "--release", "--bin", "bevy_bench_fair", "--bin", "bevy_bench_ecs"], { cwd: BEVY }),
  );
  stage("Building flecs runner", () => {
    if (!run("cmake", ["-S", ".", "-B", "build"], { cwd: FLECS })) return false;
    return run("cmake", ["--build", "build", "--config", "Release", "--target", "flecs_fair"], { cwd: FLECS });
  });
}

stage("Correctness checks", () =>
  run(path.join(ROOT, "target/release/verify_render_cache"), [], { cwd: ROOT }),
);

const onlyArgs = only ? ["--only", only] : [];
const commonArgs = ["--reps", REPS, "--warmup", WARMUP, "--scale", SCALE, ...onlyArgs];

stage("Neutral suite — Artisan", () =>
  run(path.join(ROOT, "target/release/bench_fair"), [...commonArgs, "--out", path.join(RESULTS, "fair_artisan.json")], { cwd: ROOT }),
);

stage("Neutral suite — Bevy 0.18.1", () => {

  const env = { ...process.env };
  const dirs = [path.join(BEVY, "target/release"), path.join(BEVY, "target/release/deps")];
  env.PATH = dirs.join(path.delimiter) + path.delimiter + env.PATH;
  env.LD_LIBRARY_PATH = dirs.join(":") + ":" + (env.LD_LIBRARY_PATH ?? "");
  return run(path.join(BEVY, "target/release/bevy_bench_fair"), [...commonArgs, "--out", path.join(RESULTS, "fair_bevy.json")], { cwd: BEVY, env });
});

stage("Neutral suite — flecs", () =>
  run(path.join(FLECS, "build/Release/flecs_fair"), [...commonArgs, "--out", path.join(RESULTS, "fair_flecs.json")], { cwd: FLECS }),
);

if (!only) {
  stage("Paired suite — Artisan (serial)", () =>
    run(path.join(ROOT, "target/release/bench_ecs"), ["--serial", "--reps", REPS, "--warmup", WARMUP, "--scale", SCALE, "--out", path.join(RESULTS, "artisan_ecs_serial.json")], { cwd: ROOT }),
  );
  stage("Paired suite — Artisan (parallel)", () =>
    run(path.join(ROOT, "target/release/bench_ecs"), ["--reps", REPS, "--warmup", WARMUP, "--scale", SCALE, "--out", path.join(RESULTS, "artisan_ecs_parallel.json")], { cwd: ROOT }),
  );
  stage("Paired suite — Bevy (parallel)", () => {
    const env = { ...process.env };
    const dirs = [path.join(BEVY, "target/release"), path.join(BEVY, "target/release/deps")];
    env.PATH = dirs.join(path.delimiter) + path.delimiter + env.PATH;
    return run(path.join(BEVY, "target/release/bevy_bench_ecs"), ["--reps", REPS, "--warmup", WARMUP, "--scale", SCALE, "--out", path.join(RESULTS, "bevy_ecs_parallel.json")], { cwd: BEVY, env });
  });
}

stage("Validating cross-engine equivalence", () =>
  run(process.execPath, [path.join(__dirname, "verify_fair.js")], { cwd: ROOT }),
);

stage("Rebuilding site", () =>
  run(process.execPath, [path.join(__dirname, "build_site.js")], { cwd: ROOT }),
);

const mins = ((Date.now() - t0) / 60000).toFixed(1);
console.log("\n" + "─".repeat(64));
if (failures.length) {
  console.error(`\x1b[31m${failures.length} stage(s) failed:\x1b[0m ${failures.join(", ")}`);
  console.log(`Completed in ${mins} min`);
  process.exitCode = 1;
} else {
  console.log(`\x1b[32mAll stages completed\x1b[0m in ${mins} min`);
  console.log(`\nResults written to results/ directory.`);
}
if (quick) {
  console.log(
    `\n\x1b[33mNote:\x1b[0m --quick used ${REPS} reps at scale ${SCALE}. These numbers are for
      checking that the pipeline works, not for reporting.`,
  );
}
