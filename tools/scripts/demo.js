const { execFileSync, spawn } = require("child_process");
const fs = require("fs");
const path = require("path");

const ROOT = path.join(__dirname, "../..");
const DEMOS_DIR = path.join(ROOT, "demos");
const PORT = 8000;

const name = process.argv[2];
if (!name) {
  const available = fs.readdirSync(DEMOS_DIR).filter((d) =>
    fs.existsSync(path.join(DEMOS_DIR, d, "Cargo.toml"))
  );
  console.error("Usage: node tools/scripts/demo.js <demo_name>");
  console.error(`Available demos: ${available.join(", ")}`);
  process.exit(1);
}

const demoDir = path.join(DEMOS_DIR, name);
if (!fs.existsSync(path.join(demoDir, "Cargo.toml"))) {
  console.error(`No demo named "${name}" found in ${DEMOS_DIR}`);
  process.exit(1);
}

function newestMtime(dir, exts) {
  let newest = 0;
  if (!fs.existsSync(dir)) return newest;
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === "pkg" || entry.name === "target" || entry.name === "node_modules") continue;
      newest = Math.max(newest, newestMtime(full, exts));
    } else if (exts.includes(path.extname(entry.name))) {
      newest = Math.max(newest, fs.statSync(full).mtimeMs);
    }
  }
  return newest;
}

const wasmOut = path.join(demoDir, "pkg", `${name}_bg.wasm`);
const sourceExts = [".rs", ".toml"];
const sourceNewest = Math.max(
  newestMtime(demoDir, sourceExts),
  newestMtime(path.join(ROOT, "src"), sourceExts),
  fs.existsSync(path.join(ROOT, "Cargo.toml")) ? fs.statSync(path.join(ROOT, "Cargo.toml")).mtimeMs : 0
);
const wasmMtime = fs.existsSync(wasmOut) ? fs.statSync(wasmOut).mtimeMs : 0;

if (wasmMtime < sourceNewest) {
  console.log(`[demo] Building ${name} (source changed since last build)...`);
  execFileSync("wasm-pack", ["build", `demos/${name}`, "--target", "web", "--release"], {
    cwd: ROOT,
    stdio: "inherit",
    shell: true,
  });
} else {
  console.log(`[demo] ${name} is up to date, skipping build.`);
}

console.log("[demo] Starting dev server...");
const server = spawn(process.execPath, ["tools/scripts/dev_server.js"], {
  cwd: ROOT,
  stdio: "inherit",
});

const url = `http://localhost:${PORT}/demos/${name}/index.html`;

setTimeout(() => {
  console.log(`[demo] Opening ${url}`);
  spawn("cmd", ["/c", "start", "", url], { stdio: "ignore", detached: true }).unref();
}, 600);

process.on("SIGINT", () => {
  server.kill();
  process.exit(0);
});
