// Takes the README screenshots: the real UI (Vite dev server on port 1420)
// in headless Chromium, with the Tauri backend replaced by ./mock.js, which
// only holds invented content. See CONTRIBUTING.md ("Screenshots").
//
//   bun run dev &                                   # in another terminal
//   node scripts/screenshots/run.mjs                # -> docs/screenshots
//   node scripts/screenshots/run.mjs --out /tmp/shots --only 03,07
//
// Playwright is not a dependency of the project. Set PLAYWRIGHT_RESOLVE_FROM
// to a directory whose node_modules has `playwright` (or `playwright-core`).
// CHROMIUM_PATH points at a Chromium binary when Playwright's own does not run
// (on NixOS: `nix shell nixpkgs#chromium`).
import { createRequire } from "node:module";
import { mkdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../..");

const require = createRequire(path.join(process.env.PLAYWRIGHT_RESOLVE_FROM ?? root, "noop.js"));
const { chromium } = (() => {
  try {
    return require("playwright");
  } catch {
    return require("playwright-core");
  }
})();

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 ? args[i + 1] : fallback;
};
const out = path.resolve(option("out", path.join(root, "docs/screenshots")));
const only = option("only", "")
  .split(",")
  .filter(Boolean);
const url = option("url", "http://localhost:1420/");
mkdirSync(out, { recursive: true });

// A Wednesday, so the daily text and "this week" are the same on every run.
const NOW = new Date("2026-10-07T09:30:00Z");
const FAVORITES = ["lbn_E", "mlc26_E", "mwb_E_202610", "lsq_E_202610", "prb_E"];

const rail = (page, name) => page.getByRole("button", { name, exact: true }).click();
const settle = async (page) => {
  await page.waitForFunction(() => [...document.images].every((i) => i.complete && i.naturalWidth > 0));
  await page.waitForTimeout(500);
};

const scenes = [
  ["01-home", async (page) => {
    await page.getByText("Teaching Toolbox").waitFor();
    await page.getByRole("button", { name: /Wednesday, October 7/ }).waitFor();
    await page.getByText("What's New").first().waitFor();
  }],
  ["02-daily-text-reference", async (page) => {
    await page.getByRole("button", { name: /Wednesday, October 7/ }).click();
    await page.locator(".reader .themeScrp").waitFor();
    await page.locator(".reader .themeScrp a.b").click();
    await page.getByText("Parallel Translations").waitFor();
    await page.getByText("Carry the lamp ahead of you", { exact: false }).nth(1).waitFor();
    await page.getByText("Take the lamp in front of you").waitFor();
  }],
  ["03-bible-books", async (page) => {
    await rail(page, "Bible");
    await page.getByRole("button", { name: "Lanterns", exact: true }).waitFor();
  }],
  ["03-bible-chapters", async (page) => {
    await rail(page, "Bible");
    await page.getByRole("button", { name: "Lanterns", exact: true }).click();
    await page.getByRole("button", { name: "10", exact: true }).waitFor();
  }],
  ["03-bible-study-pane", async (page) => {
    await rail(page, "Bible");
    await page.getByRole("button", { name: "Lanterns", exact: true }).click();
    await page.getByRole("button", { name: "10", exact: true }).click();
    await page.locator(".reader.bible mark.hl").first().waitFor();
    await page.getByText("Outline of Lanterns").waitFor();
    // Point the pane at verse 4: footnote, study note and Research Guide.
    await page.locator("#v2-10-4-1 .vl").click();
  }],
  ["04-library", async (page) => {
    await rail(page, "Library");
    await page.getByText("Brochures and Booklets").waitFor();
  }],
  ["04-library-video", async (page) => {
    await rail(page, "Library");
    await page.getByRole("tab", { name: "Video", exact: true }).click();
    await page.getByText("Gatherings and Events").waitFor();
  }],
  ["05-meetings", async (page) => {
    await rail(page, "Meetings");
    await page.getByText("Why Small Kindnesses Outlast Grand Gestures").first().waitFor();
    await page.getByText("Other Meeting Publications").waitFor();
  }],
  ["06-workbook-highlights", async (page) => {
    await rail(page, "Meetings");
    await page.getByRole("button", { name: "October 5-11", exact: true }).click();
    await page.locator(".reader mark.hl").first().waitFor();
    await page.getByText("My notes").waitFor();
  }],
  ["07-highlight-toolbar", async (page) => {
    await rail(page, "Meetings");
    await page.getByRole("button", { name: "October 5-11", exact: true }).click();
    await page.locator('.reader mark.hl[data-color="3"]').first().click();
    await page.getByRole("button", { name: "Highlight Pink" }).waitFor();
  }],
  ["08-personal-study", async (page) => {
    await rail(page, "Personal Study");
    await page.getByText("Patient listening").waitFor();
    await page.getByText("Bookmarks", { exact: true }).waitFor();
  }],
  ["09-settings", async (page) => {
    await rail(page, "Settings");
    await page.getByText("Interface language").waitFor();
    await page.getByText("Version 1.5.0").waitFor();
    // The text-size sample is a fixed line of Scripture in the UI; show an invented one.
    await page.locator("section:has(input[type=range]) p.reader").evaluate((el) => {
      el.textContent = "A lamp lit at dawn needs no defence; the daylight speaks for it.";
    });
  }],
].filter(([name]) => only.length === 0 || only.some((o) => name.startsWith(o)));

const browser = await chromium.launch({ executablePath: process.env.CHROMIUM_PATH || undefined });
const failures = [];
const missing = new Set();

for (const scheme of ["light", "dark"]) {
  const context = await browser.newContext({
    viewport: { width: 1320, height: 860 },
    locale: "en-US",
    timezoneId: "UTC",
    colorScheme: scheme,
    deviceScaleFactor: 1,
  });
  await context.addInitScript({ path: path.join(here, "mock.js") });
  await context.addInitScript(
    ({ theme, favorites }) => {
      localStorage.setItem("theme", theme);
      localStorage.setItem("lang", "E");
      localStorage.setItem("uiLang", "en");
      localStorage.setItem("favorites", JSON.stringify(favorites));
    },
    { theme: scheme, favorites: FAVORITES },
  );
  await context.clock.setFixedTime(NOW);

  for (const [name, run] of scenes) {
    const page = await context.newPage();
    page.on("pageerror", (e) => console.log(`  [${name}-${scheme}] page error: ${e.message}`));
    try {
      await page.goto(url);
      await run(page);
      await settle(page);
      await page.mouse.move(1310, 5);
      await page.screenshot({ path: path.join(out, `${name}-${scheme}.png`) });
      console.log(`ok   ${name}-${scheme}`);
    } catch (e) {
      failures.push(`${name}-${scheme}`);
      console.log(`FAIL ${name}-${scheme}: ${String(e.message).split("\n")[0]}`);
      await page.screenshot({ path: path.join(out, `_failed-${name}-${scheme}.png`) }).catch(() => undefined);
    }
    for (const m of await page.evaluate(() => window.__mockMissing ?? []).catch(() => [])) missing.add(m);
    await page.close();
  }
  await context.close();
}
await browser.close();

if (missing.size > 0) console.log(`unmocked commands: ${[...missing].join(", ")}`);
if (failures.length > 0) {
  console.log(`failed: ${failures.join(", ")}`);
  process.exit(1);
}
