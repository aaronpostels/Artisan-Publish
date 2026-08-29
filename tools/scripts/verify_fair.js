#!/usr/bin/env node

const fs = require("fs");
const path = require("path");

const ROOT = path.join(__dirname, "../..");
const RESULTS = path.join(ROOT, "results");
const ENGINES = ["artisan", "bevy", "flecs"];

function load(name) {
  const p = path.join(RESULTS, `fair_${name}.json`);
  if (!fs.existsSync(p)) return null;
  return JSON.parse(fs.readFileSync(p, "utf8"));
}

const runs = {};
for (const e of ENGINES) {
  const r = load(e);
  if (r) runs[e] = r;
}

const present = Object.keys(runs);
if (!present.length) {
  console.error("No fair_*.json result files found. Run `npm run bench:all` first.");
  process.exit(1);
}
if (!runs.artisan) {
  console.error("fair_artisan.json missing — nothing to compare against.");
  process.exit(1);
}

const key = (r) => `${r.id}@${r.sweep_value}`;

const index = {};
for (const e of present) {
  index[e] = new Map(runs[e].results.map((r) => [key(r), r]));
}

const categories = runs.artisan.results.map((r) => ({
  key: key(r),
  id: r.id,
  group: r.group,
  label: r.label,
  sweep_key: r.sweep_key,
  sweep_value: r.sweep_value,
  description: r.description,
}));

const rows = [];
let invalid = 0;

for (const c of categories) {
  const row = { ...c, engines: {}, valid: true, notes: [] };

  const checksums = new Map();
  for (const e of present) {
    const r = index[e].get(c.key);
    if (!r) {
      row.engines[e] = { missing: true };
      continue;
    }
    if (r.unsupported) {
      row.engines[e] = { unsupported: true, reason: r.unsupported_reason };
      row.notes.push(`${e}: unsupported`);
      continue;
    }
    row.engines[e] = {
      median: r.stats.median,
      ci95: r.stats.ci95_median,
      rsd: r.stats.rsd,
      min: r.stats.min,
      samples: r.stats.samples,
      checksum_stable: r.checksum_stable,
    };
    if (!r.checksum_stable) row.notes.push(`${e}: unstable checksum`);
    checksums.set(e, r.checksum);
  }

  const distinct = new Set(checksums.values());
  if (distinct.size > 1) {
    row.valid = false;
    row.checksum_mismatch = Object.fromEntries(checksums);
    row.notes.push("checksum mismatch");
    invalid++;
  } else {
    row.checksum = [...distinct][0] ?? null;
  }

  const timed = present.filter((e) => row.engines[e]?.median != null);
  if (row.valid && timed.length > 1) {
    const best = Math.min(...timed.map((e) => row.engines[e].median));
    for (const e of timed) row.engines[e].relative = row.engines[e].median / best;
    row.fastest = timed.find((e) => row.engines[e].median === best);
  }

  rows.push(row);
}

const summary = {};
for (const e of present) {
  const ratios = rows
    .filter((r) => r.valid && r.engines[e]?.relative != null)
    .map((r) => r.engines[e].relative);
  summary[e] = {
    categories: ratios.length,
    geomean_vs_best:
      ratios.length > 0
        ? Math.exp(ratios.reduce((a, b) => a + Math.log(b), 0) / ratios.length)
        : null,
    wins: rows.filter((r) => r.valid && r.fastest === e).length,
  };
}

const artisanLosses = rows
  .filter((r) => r.valid && r.fastest && r.fastest !== "artisan" && r.engines.artisan?.relative)
  .map((r) => ({
    id: r.id,
    label: r.label,
    group: r.group,
    behind: r.engines.artisan.relative,
    fastest: r.fastest,
  }))
  .sort((a, b) => b.behind - a.behind);

const out = {
  generated: new Date().toISOString(),
  engines: present,
  env: Object.fromEntries(present.map((e) => [e, runs[e].env])),
  cfg: runs.artisan.cfg,
  summary,
  artisan_losses: artisanLosses,
  invalid_categories: invalid,
  rows,
};

fs.writeFileSync(path.join(RESULTS, "fair_summary.json"), JSON.stringify(out, null, 2));

if (process.argv.includes("--json")) {
  console.log(JSON.stringify(out.summary, null, 2));
  process.exit(invalid ? 1 : 0);
}

console.log(`\nNeutral suite — ${present.join(", ")}  (${rows.length} categories)\n`);

let group = "";
for (const r of rows) {
  if (r.group !== group) {
    group = r.group;
    console.log(`  ${group.toUpperCase()}`);
  }
  const cells = present
    .map((e) => {
      const v = r.engines[e];
      if (!v) return "—".padStart(11);
      if (v.unsupported) return "n/a".padStart(11);
      return v.median.toFixed(3).padStart(11);
    })
    .join("");
  const flag = !r.valid ? "  ✖ CHECKSUM MISMATCH" : r.notes.length ? `  (${r.notes.join(", ")})` : "";
  console.log(`    ${r.label.padEnd(30)}${cells}${flag}`);
}

console.log(`\n  ${"".padEnd(30)}${present.map((e) => e.padStart(11)).join("")}`);
console.log("\nSummary (geometric mean of per-category ratios vs the fastest engine; 1.00 = fastest everywhere)");
for (const e of present) {
  const s = summary[e];
  console.log(
    `  ${e.padEnd(10)} ${s.geomean_vs_best ? s.geomean_vs_best.toFixed(2) + "×" : "—"}`.padEnd(20) +
      `  fastest in ${s.wins}/${s.categories} categories`,
  );
}

if (artisanLosses.length) {
  console.log(`\nWhere Artisan is not fastest (${artisanLosses.length}):`);
  for (const l of artisanLosses.slice(0, 12)) {
    console.log(`  ${l.label.padEnd(32)} ${l.behind.toFixed(2)}× behind ${l.fastest}`);
  }
} else {
  console.log("\nArtisan is fastest in every valid category.");
}

if (invalid) {
  console.error(`\n✖ ${invalid} categor(y/ies) had mismatched checksums and are reported as invalid.`);
  process.exitCode = 1;
}

console.log(`\nWrote results/fair_summary.json`);
