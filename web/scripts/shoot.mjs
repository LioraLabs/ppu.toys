#!/usr/bin/env node
// Screenshot a Cosmos fixture from the static export.
// Usage: node scripts/shoot.mjs <component-path[#variant]> [--build] [--out path]

import { spawn } from "node:child_process";
import * as fs from "node:fs";
import { createServer } from "node:http";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
import { resolveFixtureRef } from "./fixture-ref.mjs";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const webRoot = path.resolve(scriptDir, "..");
const buildRoot = path.join(webRoot, "build");
const manifestPath = path.join(buildRoot, "cosmos.fixtures.json");

const MIME_TYPES = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".css": "text/css",
  ".json": "application/json",
  ".wasm": "application/wasm",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".ico": "image/x-icon",
};

function fail(message) {
  console.error(message);
  process.exit(1);
}

function parseArgs(argv) {
  const args = { ref: null, out: null, build: false, width: 1280, height: 800, theme: null };
  const rest = [...argv];
  while (rest.length) {
    const token = rest.shift();
    if (token === "--out") args.out = rest.shift();
    else if (token === "--build") args.build = true;
    else if (token === "--width") args.width = Number.parseInt(rest.shift(), 10);
    else if (token === "--height") args.height = Number.parseInt(rest.shift(), 10);
    else if (token === "--theme") args.theme = rest.shift();
    else if (token.startsWith("--")) fail(`Unknown flag: ${token}`);
    else if (!args.ref) args.ref = token;
    else fail(`Unexpected extra argument: ${token}`);
  }
  if (!args.ref) fail("Usage: npm run shoot -- <component-path[#variant]> [--build] [--out path]");
  if (args.theme && !["light", "dark"].includes(args.theme)) fail(`Invalid theme: ${args.theme}`);
  if (!Number.isInteger(args.width) || !Number.isInteger(args.height)) fail("Width and height must be integers");
  return args;
}

function runExport() {
  return new Promise((resolve, reject) => {
    console.error("Running `npm run cosmos:export` ...");
    const child = spawn("npm", ["run", "cosmos:export"], { cwd: webRoot, stdio: "inherit" });
    child.on("error", reject);
    child.on("exit", (code) => (code === 0 ? resolve() : reject(new Error(`Cosmos export exited with ${code}`))));
  });
}

async function loadManifest(forceBuild) {
  if (forceBuild || !fs.existsSync(manifestPath)) await runExport();
  if (!fs.existsSync(manifestPath)) fail(`Missing ${manifestPath}; run npm run cosmos:export`);
  return JSON.parse(fs.readFileSync(manifestPath, "utf8"));
}

function startStaticServer(root) {
  return new Promise((resolve, reject) => {
    const server = createServer((req, res) => {
      const url = new URL(req.url, "http://localhost");
      const requestPath = decodeURIComponent(url.pathname === "/" ? "/index.html" : url.pathname);
      const resolvedPath = path.normalize(path.join(root, requestPath));
      const relative = path.relative(root, resolvedPath);
      if (relative.startsWith("..") || path.isAbsolute(relative)) {
        res.writeHead(403).end("Forbidden");
        return;
      }
      fs.readFile(resolvedPath, (error, data) => {
        if (error) {
          res.writeHead(404).end("Not found");
          return;
        }
        res.writeHead(200, { "Content-Type": MIME_TYPES[path.extname(resolvedPath)] || "application/octet-stream" });
        res.end(data);
      });
    });
    server.on("error", reject);
    server.listen(0, "127.0.0.1", () => resolve(server));
  });
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const manifest = await loadManifest(args.build);
  let fixtureId;
  try {
    fixtureId = resolveFixtureRef(manifest, args.ref);
  } catch (error) {
    console.error(error.message);
    console.error("Available component paths:");
    for (const fixture of manifest.fixtures) {
      console.error(`  ${fixture.filePath.replace(/^src\//, "").replace(/\.fixture\.tsx$/, "")}`);
    }
    process.exit(1);
  }

  const safeRef = args.ref.replace(/[^a-zA-Z0-9_.-]+/g, "--");
  const outPath = path.resolve(webRoot, args.out || path.join(".shots", `${safeRef}.png`));
  fs.mkdirSync(path.dirname(outPath), { recursive: true });

  let server;
  let browser;
  try {
    server = await startStaticServer(buildRoot);
    const { port } = server.address();
    const query = encodeURIComponent(JSON.stringify(fixtureId));
    const url = `http://127.0.0.1:${port}/renderer.html?fixtureId=${query}&locked=true`;
    const { chromium } = await import("playwright");
    browser = await chromium.launch();
    const page = await browser.newPage({ viewport: { width: args.width, height: args.height } });
    await page.goto(url);
    await page.waitForSelector("body[data-cosmos-ready='true']");
    if (args.theme) await page.evaluate((theme) => (document.documentElement.dataset.theme = theme), args.theme);
    await page.locator("#root > *").first().screenshot({ path: outPath });
    console.log(outPath);
    console.log(`${fs.statSync(outPath).size} bytes`);
  } catch (error) {
    fail(`shoot failed: ${error?.stack || error}`);
  } finally {
    if (browser) await browser.close();
    if (server) await new Promise((resolve) => server.close(resolve));
  }
}

main().catch((error) => fail(`shoot failed: ${error?.stack || error}`));
