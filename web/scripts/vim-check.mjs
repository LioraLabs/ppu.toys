import { chromium } from "playwright";

const BASE = process.argv[2] || "http://localhost:5173";
const RUNS = Number(process.argv[3] || 5);

const status = (page) =>
  page.evaluate(() =>
    [...document.querySelectorAll(".cm-panel")].map((p) => p.textContent).join("|"),
  );

async function scenario(page, { vimAtLoad }) {
  await page.addInitScript(
    (on) => {
      localStorage.setItem("ppu.editor.vim", on);
      localStorage.removeItem("ppu.dockLayout.v3");
    },
    vimAtLoad ? "1" : "0",
  );
  await page.goto(BASE + "/studio");
  await page.waitForSelector(".cm-content", { timeout: 15000 });
  await page.waitForTimeout(500);

  if (!vimAtLoad) {
    await page.click("button:has-text('vim off')");
    await page.waitForTimeout(200);
  }
  const s0 = await status(page);
  if (!s0.includes("NORMAL")) return `no NORMAL after enable (got "${s0}")`;

  await page.click(".cm-content");
  await page.keyboard.press("j");
  await page.keyboard.press("i");
  await page.waitForTimeout(100);
  const s1 = await status(page);
  if (!s1.includes("INSERT")) return `i did not enter insert (got "${s1}")`;

  await page.keyboard.type("ZZ");
  await page.keyboard.press("Escape");
  await page.waitForTimeout(100);
  const s2 = await status(page);
  if (!s2.includes("NORMAL")) return `Escape did not exit insert (got "${s2}")`;

  await page.keyboard.press("u");
  await page.waitForTimeout(100);
  const doc = await page.evaluate(() =>
    [...document.querySelectorAll(".cm-line")].map((l) => l.textContent).join("\n"),
  );
  if (doc.includes("ZZ")) return `u did not undo (doc still has ZZ)`;

  // dd then u
  await page.keyboard.press("d");
  await page.keyboard.press("d");
  await page.keyboard.press("u");
  await page.waitForTimeout(100);

  // ex command line
  await page.keyboard.press("Shift+Semicolon");
  await page.waitForTimeout(150);
  const hasDialog = await page.evaluate(() => !!document.querySelector(".cm-panel input"));
  if (!hasDialog) return "':' did not open ex command line";
  await page.keyboard.press("Escape");
  return null;
}

const browser = await chromium.launch();
let failures = 0;
for (const vimAtLoad of [true, false]) {
  for (let run = 1; run <= RUNS; run++) {
    const page = await browser.newPage();
    let err;
    try {
      err = await scenario(page, { vimAtLoad });
    } catch (e) {
      err = e.message.split("\n")[0];
    }
    const label = `${vimAtLoad ? "vim-at-load" : "toggle-on"} #${run}`;
    console.log(err ? `FAIL ${label}: ${err}` : `pass ${label}`);
    if (err) failures++;
    await page.close();
  }
}
await browser.close();
console.log(failures ? `${failures} FAILURES` : "ALL PASS");
process.exit(failures ? 1 : 0);
