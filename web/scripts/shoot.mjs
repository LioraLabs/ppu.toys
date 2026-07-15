#!/usr/bin/env node
// Headless screenshot harness for Ladle stories.
//
// Usage:
//   node scripts/shoot.mjs <story-id> [--out <path>] [--build] [--width N] [--height N] [--theme light|dark]
//
// Renders a single Ladle story (from the static `ladle build` output) to a PNG
// using Playwright Chromium, without booting the full app or wasm.

import { spawn } from "node:child_process";
import * as fs from "node:fs";
import { createServer } from "node:http";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const webRoot = path.resolve(__dirname, "..");
const buildRoot = path.join(webRoot, "build");
const metaPath = path.join(buildRoot, "meta.json");

const MIME_TYPES = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".mjs": "text/javascript",
  ".css": "text/css",
  ".json": "application/json",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".webmanifest": "application/manifest+json",
  ".ico": "image/x-icon",
};

function fail(message) {
  console.error(message);
  process.exit(1);
}

function parseArgs(argv) {
  const args = { storyId: null, out: null, build: false, width: 1280, height: 800, theme: null };
  const rest = [...argv];

  while (rest.length > 0) {
    const token = rest.shift();
    switch (token) {
      case "--out":
        args.out = rest.shift();
        break;
      case "--build":
        args.build = true;
        break;
      case "--width":
        args.width = Number.parseInt(rest.shift(), 10);
        break;
      case "--height":
        args.height = Number.parseInt(rest.shift(), 10);
        break;
      case "--theme":
        args.theme = rest.shift();
        break;
      default:
        if (token.startsWith("--")) {
          fail(`Unknown flag: ${token}`);
        } else if (args.storyId === null) {
          args.storyId = token;
        } else {
          fail(`Unexpected extra argument: ${token}`);
        }
    }
  }

  if (!args.storyId) {
    fail(
      "Usage: node scripts/shoot.mjs <story-id> [--out <path>] [--build] [--width N] [--height N] [--theme light|dark]",
    );
  }
  if (args.theme && args.theme !== "light" && args.theme !== "dark") {
    fail(`Invalid --theme value: ${args.theme} (expected "light" or "dark")`);
  }
  if (Number.isNaN(args.width) || Number.isNaN(args.height)) {
    fail("Invalid --width/--height: must be integers");
  }

  return args;
}

function runLadleBuild() {
  return new Promise((resolve, reject) => {
    console.error("Running `npx ladle build` ...");
    const child = spawn("npx", ["ladle", "build"], { cwd: webRoot, stdio: "inherit" });
    child.on("error", reject);
    child.on("exit", (code) => {
      if (code === 0) {
        resolve();
      } else {
        reject(new Error(`ladle build exited with code ${code}`));
      }
    });
  });
}

async function ensureMeta(forceBuild) {
  if (forceBuild || !fs.existsSync(metaPath)) {
    await runLadleBuild();
  }
  if (!fs.existsSync(metaPath)) {
    fail(`Could not find ${metaPath} even after running ladle build.`);
  }
  const raw = fs.readFileSync(metaPath, "utf8");
  return JSON.parse(raw);
}

function startStaticServer(root) {
  return new Promise((resolve, reject) => {
    const server = createServer((req, res) => {
      try {
        const url = new URL(req.url, "http://localhost");
        let requestPath = decodeURIComponent(url.pathname);
        if (requestPath === "/") {
          requestPath = "/index.html";
        }

        const resolvedPath = path.normalize(path.join(root, requestPath));
        const relative = path.relative(root, resolvedPath);
        const isInsideRoot = relative === "" || (!relative.startsWith("..") && !path.isAbsolute(relative));
        if (!isInsideRoot) {
          res.writeHead(403, { "Content-Type": "text/plain" });
          res.end("Forbidden");
          return;
        }

        fs.readFile(resolvedPath, (err, data) => {
          if (err) {
            res.writeHead(404, { "Content-Type": "text/plain" });
            res.end("Not found");
            return;
          }
          const ext = path.extname(resolvedPath).toLowerCase();
          const contentType = MIME_TYPES[ext] || "application/octet-stream";
          res.writeHead(200, { "Content-Type": contentType });
          res.end(data);
        });
      } catch (err) {
        res.writeHead(500, { "Content-Type": "text/plain" });
        res.end("Internal server error");
      }
    });

    server.on("error", reject);
    server.listen(0, "127.0.0.1", () => {
      resolve(server);
    });
  });
}

async function main() {
  const args = parseArgs(process.argv.slice(2));

  const meta = await ensureMeta(args.build);
  const storyIds = Object.keys(meta.stories ?? {});
  if (!storyIds.includes(args.storyId)) {
    console.error(`Unknown story id: "${args.storyId}"`);
    console.error("Valid story ids:");
    for (const id of storyIds.sort()) {
      console.error(`  ${id}`);
    }
    process.exit(1);
  }

  const outPath = args.out
    ? path.resolve(webRoot, args.out)
    : path.join(webRoot, ".shots", `${args.storyId}.png`);
  fs.mkdirSync(path.dirname(outPath), { recursive: true });

  let server;
  let browser;
  try {
    server = await startStaticServer(buildRoot);
    const { port } = server.address();

    let url = `http://127.0.0.1:${port}/?story=${encodeURIComponent(args.storyId)}&mode=preview`;
    if (args.theme) {
      url += `&theme=${args.theme}`;
    }

    const { chromium } = await import("playwright");
    browser = await chromium.launch();
    const page = await browser.newPage({ viewport: { width: args.width, height: args.height } });
    await page.goto(url);
    await page.waitForSelector("[data-storyloaded]");
    // Screenshot the story-content container (`#ladle-root`) rather than the
    // fixed viewport, so the capture tightly bounds the rendered component: no
    // viewport whitespace for small components, and no clipping for ones taller
    // than the viewport. Fall back to a full-page shot if the container is
    // missing so tall content is still never clipped.
    const storyEl = await page.$("#ladle-root");
    if (storyEl) {
      await storyEl.screenshot({ path: outPath });
    } else {
      await page.screenshot({ path: outPath, fullPage: true });
    }

    const { size } = fs.statSync(outPath);
    console.log(outPath);
    console.log(`${size} bytes`);
  } catch (err) {
    fail(`shoot failed: ${err && err.stack ? err.stack : err}`);
  } finally {
    if (browser) {
      await browser.close();
    }
    if (server) {
      await new Promise((resolve) => server.close(resolve));
    }
  }
}

main().catch((err) => {
  fail(`shoot failed: ${err && err.stack ? err.stack : err}`);
});
