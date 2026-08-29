#!/usr/bin/env node

const fs = require("fs");

const args = process.argv.slice(2);
const asMarkdown = args.includes("--md");
const [pathA, pathB] = args.filter((a) => !a.startsWith("--"));

if (!pathA || !pathB) {
  console.error("usage: compare_results.js <a.json> <b.json> [--md]");
  process.exit(2);
}

const a = JSON.parse(fs.readFileSync(pathA, "utf8"));
const b = JSON.parse(fs.readFileSync(pathB, "utf8"));

for (const [p, r] of [
  [pathA, a],
  [pathB, b],
]) {
  if (r.schema_version !== 2) {
    console.error(`${p}: schema_version ${r.schema_version}, expected 2`);
    process.exit(2);
  }
}

const problems = [];
if (a.cfg.parallel !== b.cfg.parallel) {
  problems.push(
    `parallelism differs: ${pathA} is ${a.cfg.parallel ? "parallel" : "serial"}, ` +
      `${pathB} is ${b.cfg.parallel ? "parallel" : "serial"}. ` +
      `Comparing them would change the ECS implementation and the execution strategy at once.`,
  );
}
if (a.cfg.work_scale !== b.cfg.work_scale) {
  problems.push(
    `work scale differs: ${a.cfg.work_scale} vs ${b.cfg.work_scale} — different workloads.`,
  );
}

const bByName = new Map(b.results.map((r) => [r.name, r]));
const rows = [];
let mismatches = 0;

for (const ra of a.results) {
  const rb = bByName.get(ra.name);
  if (!rb) continue;

  const same = ra.checksum === rb.checksum;
  if (!same) mismatches++;

  const unstable = !ra.checksum_stable || !rb.checksum_stable;
  const noisy = ra.stats.rsd > 0.1 || rb.stats.rsd > 0.1;

  rows.push({
    name: ra.name,
    entities: ra.entity_count,
    aMed: ra.stats.median,
    bMed: rb.stats.median,
    ratio: same && !problems.length ? rb.stats.median / ra.stats.median : null,
    aRsd: ra.stats.rsd,
    bRsd: rb.stats.rsd,
    flags: [
      same ? null : "CHECKSUM MISMATCH",
      unstable ? "unstable checksum" : null,
      noisy ? "RSD>10%" : null,
    ].filter(Boolean),
  });
}

const nameA = a.framework ?? "A";
const nameB = b.framework ?? "B";

if (asMarkdown) {
  console.log(`| Category | Entities | ${nameA} median (ms) | ${nameB} median (ms) | ${nameB}/${nameA} | Notes |`);
  console.log(`|---|---:|---:|---:|---:|---|`);
  for (const r of rows) {
    console.log(
      `| ${r.name} | ${r.entities.toLocaleString()} | ${r.aMed.toFixed(3)} | ${r.bMed.toFixed(3)} | ` +
        `${r.ratio === null ? "—" : r.ratio.toFixed(2) + "×"} | ${r.flags.join(", ")} |`,
    );
  }
} else {
  console.log(`\n${pathA}  (${nameA}, ${a.cfg.parallel ? "parallel" : "serial"})`);
  console.log(`${pathB}  (${nameB}, ${b.cfg.parallel ? "parallel" : "serial"})\n`);
  console.log(
    "CATEGORY".padEnd(28) +
      nameA.padStart(12) +
      nameB.padStart(12) +
      "RATIO".padStart(9) +
      "  NOTES",
  );
  for (const r of rows) {
    console.log(
      r.name.padEnd(28) +
        r.aMed.toFixed(2).padStart(12) +
        r.bMed.toFixed(2).padStart(12) +
        (r.ratio === null ? "—" : r.ratio.toFixed(2) + "×").padStart(9) +
        "  " +
        r.flags.join(", "),
    );
  }
}

if (problems.length) {
  console.error("\nRatios suppressed:");
  for (const p of problems) console.error(`  - ${p}`);
}
if (mismatches) {
  console.error(
    `\n${mismatches} categor${mismatches === 1 ? "y" : "ies"} produced differing checksums. ` +
      `Those runs did not perform the same work.`,
  );
}
if (problems.length || mismatches) process.exitCode = 1;

if (!asMarkdown) {
  const line = (label, va, vb) =>
    console.log(`  ${label.padEnd(22)} ${String(va).padEnd(46)} ${vb}`);
  console.log("\nEnvironment");
  line("cpu", a.env.cpu_brand, b.env.cpu_brand);
  line("os", a.env.os_description, b.env.os_description);
  line("threads", a.env.rayon_threads ?? a.env.available_parallelism, b.env.rayon_threads ?? b.env.available_parallelism);
  line("warmup/reps", `${a.cfg.warmup}/${a.cfg.reps}`, `${b.cfg.warmup}/${b.cfg.reps}`);
  line("commit", `${a.env.git_commit ?? "—"}${a.env.git_dirty ? " (dirty)" : ""}`, `${b.env.git_commit ?? "—"}${b.env.git_dirty ? " (dirty)" : ""}`);
  console.log();
}
