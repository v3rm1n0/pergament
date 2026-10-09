// Translation helper, run with bun (see docs/TRANSLATING.md):
//   bun run i18n                          coverage of every language; exits 1 on errors
//   bun run i18n -- --missing             also lists the texts still to translate
//   bun run i18n:new fr "Français" fr-FR  start a new language
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { locales } from "../ui/src/locales";
import { extractKeys, registerLocale, report, scaffold, validateNewLocale } from "../ui/src/locales/tools";

const root = join(import.meta.dir, "..");
const localesDir = join(root, "ui/src/locales");

/** The English texts the source passes to `t("…")`, in the order they are found. */
function sourceKeys(dir = join(root, "ui/src")): string[] {
  const keys: string[] = [];
  for (const name of readdirSync(dir).sort()) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) {
      if (name !== "locales") keys.push(...sourceKeys(path));
    } else if (/\.tsx?$/.test(name) && !/\.test\.tsx?$/.test(name) && name !== "fixtures.ts") {
      keys.push(...extractKeys(readFileSync(path, "utf8")));
    }
  }
  return keys;
}

function check(listMissing: boolean): number {
  const reports = report(locales, sourceKeys());
  let failed = false;
  for (const r of reports) {
    const percent = r.total ? Math.round((r.translated / r.total) * 100) : 100;
    console.log(`${r.code.padEnd(8)} ${r.name.padEnd(24)} ${String(r.translated).padStart(4)}/${r.total}  ${percent}%`);
    for (const e of r.errors) console.log(`  error: ${e}`);
    if (listMissing) for (const k of r.missing) console.log(`  missing: ${JSON.stringify(k)}`);
    failed ||= r.errors.length > 0;
  }
  if (!listMissing && reports.some((r) => r.missing.length > 0)) console.log("\nRun with --missing to list what is left to translate.");
  return failed ? 1 : 0;
}

function create(code: string, name: string, tag: string): number {
  const problem = validateNewLocale(code, name, tag, locales.map((l) => l.code));
  if (problem) {
    console.error(`i18n: ${problem}`);
    return 1;
  }
  const file = join(localesDir, `${code}.ts`);
  if (existsSync(file)) {
    console.error(`i18n: ${file} exists already`);
    return 1;
  }
  // Texts in the order of the first language (it is grouped by screen), then the ones only the source has.
  const known = [...new Set([...locales.flatMap((l) => Object.keys(l.texts)), ...sourceKeys()])];
  const indexFile = join(localesDir, "index.ts");
  const index = registerLocale(readFileSync(indexFile, "utf8"), code, name, tag);
  writeFileSync(file, scaffold(code, name, known));
  writeFileSync(indexFile, index);
  console.log(`Created ui/src/locales/${code}.ts with ${known.length} texts and added it to ui/src/locales/index.ts.`);
  console.log("Fill in the empty texts (an empty one stays English), keep the {markers} as they are,");
  console.log("then run `bun run i18n` to check them and `bun run dev` or the app to look at the result.");
  return 0;
}

const [command, ...args] = process.argv.slice(2);
if (command === "new") {
  const [code, name, tag] = args;
  if (!code || !name || !tag) {
    console.error('usage: bun run i18n:new <code> "<Name in that language>" <tag>   e.g. fr "Français" fr-FR');
    process.exit(1);
  }
  process.exit(create(code, name, tag));
} else if (command === undefined || command === "--missing" || command === "check") {
  process.exit(check(command === "--missing" || args.includes("--missing")));
} else {
  console.error(`i18n: unknown command ${command}`);
  process.exit(1);
}
