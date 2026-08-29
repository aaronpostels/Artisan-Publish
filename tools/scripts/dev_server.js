const http = require("http");
const fs = require("fs");
const path = require("path");

const PORT = Number(process.env.ARTISAN_DEV_PORT || 8000);
const ROOT = path.resolve(process.env.ARTISAN_DEV_ROOT || path.join(__dirname, "../.."));

const MIME = {
  ".html": "text/html",
  ".js": "application/javascript",
  ".wasm": "application/wasm",
  ".json": "application/json",
  ".css": "text/css",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".pdf": "application/pdf",
};

const server = http.createServer((req, res) => {

  if (req.url === "/favicon.ico") {
    res.writeHead(204);
    res.end();
    return;
  }

  if (req.method === "POST" && req.url.startsWith("/__dev/save")) {
    const reqURL = new URL(req.url, `http://${req.headers.host}`);
    const name = (reqURL.searchParams.get("name") || "capture.bin").replace(/[^a-zA-Z0-9._-]/g, "_");
    const dir = path.join(ROOT, ".devshots");
    if (!fs.existsSync(dir)) fs.mkdirSync(dir);
    const chunks = [];
    req.on("data", (c) => chunks.push(c));
    req.on("end", () => {
      fs.writeFileSync(path.join(dir, name), Buffer.concat(chunks));
      res.writeHead(200, { "Content-Type": "text/plain" });
      res.end("saved " + name);
    });
    return;
  }

  const baseURL = `http://${req.headers.host}`;
  const reqURL = new URL(req.url, baseURL);
  let pathname = reqURL.pathname;

  if (pathname.endsWith("/")) pathname += "index.html";

  const filePath = path.join(ROOT, pathname.replace(/^\/+/, ""));

  if (!fs.existsSync(filePath) || fs.statSync(filePath).isDirectory()) {
    res.writeHead(404);
    res.end("Not Found");
    return;
  }

  const ext = path.extname(filePath);
  res.writeHead(200, {
    "Content-Type": MIME[ext] || "text/plain",

    "Cache-Control": "no-store, must-revalidate",
    "Cross-Origin-Opener-Policy": "same-origin",
    "Cross-Origin-Embedder-Policy": "require-corp",
  });
  res.end(fs.readFileSync(filePath));
});

server.on("error", (err) => {
  if (err.code === "EADDRINUSE") {
    console.error(`Port ${PORT} is already in use by another process.`);
    console.error(`Set ARTISAN_DEV_PORT=<port> to use a different port.`);
    process.exit(1);
  } else {
    throw err;
  }
});

server.listen(PORT, () => {
  console.log(`Dev server: http://localhost:${PORT}`);
  console.log(`COOP/COEP: ENABLED — SharedArrayBuffer is active`);
});
