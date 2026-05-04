#!/usr/bin/env tsx
/**
 * Fetches pages from production and a local dev server and compares them 1:1.
 *
 * The comparison set is built in three layers (deduped):
 *   1. One representative page per shortcode found in layouts/shortcodes/
 *   2. One representative page per top-level content section / layout type
 *   3. Random sample from the production sitemap to reach --sample total
 *
 * Usage:
 *   tsx scripts/compare-pages.ts [options]
 *
 * Options:
 *   --sample N       Cap total pages (default: unlimited — all sitemap pages)
 *   --concurrency N  Parallel fetches (default: 10)
 *   --local URL      Local server base URL (default: http://localhost:1313)
 *   --prod URL       Production base URL (default: https://vector.dev)
 *   --seed N         Random seed for reproducible page order (default: random)
 *   --verbose        Print per-class diffs even for passing pages
 */

import * as fs from "fs";
import * as path from "path";
import { execSync } from "child_process";
import { fileURLToPath } from "url";
import * as cheerio from "cheerio";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// ---------------------------------------------------------------------------
// CLI args
// ---------------------------------------------------------------------------

const args = process.argv.slice(2);
const flag = (name: string, fallback: string) => {
  const idx = args.indexOf(name);
  return idx !== -1 && args[idx + 1] ? args[idx + 1] : fallback;
};
const hasFlag = (name: string) => args.includes(name);

const SAMPLE_SIZE = args.includes("--sample") ? parseInt(flag("--sample", "80"), 10) : Infinity;
const CONCURRENCY = parseInt(flag("--concurrency", "10"), 10);
const LOCAL_BASE = flag("--local", "http://localhost:1313");
const PROD_BASE = flag("--prod", "https://vector.dev");
const SEED = parseInt(flag("--seed", String(Date.now())), 10);
const VERBOSE = hasFlag("--verbose");

// Root of the Hugo site (one level up from scripts/)
const SITE_ROOT = path.resolve(__dirname, "..");

// ---------------------------------------------------------------------------
// Seeded RNG (mulberry32)
// ---------------------------------------------------------------------------

function seededRandom(seed: number) {
  let s = seed >>> 0;
  return () => {
    s |= 0;
    s = (s + 0x6d2b79f5) | 0;
    let t = Math.imul(s ^ (s >>> 15), 1 | s);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function shuffle<T>(arr: T[], rng: () => number): T[] {
  const copy = [...arr];
  for (let i = copy.length - 1; i > 0; i--) {
    const j = Math.floor(rng() * (i + 1));
    [copy[i], copy[j]] = [copy[j], copy[i]];
  }
  return copy;
}

// ---------------------------------------------------------------------------
// Content path → URL path
// ---------------------------------------------------------------------------

function contentPathToUrl(filePath: string): string {
  // e.g. content/en/docs/setup/quickstart.md  →  /docs/setup/quickstart/
  //      content/en/docs/administration/_index.md → /docs/administration/
  return filePath
    .replace(/^.*content\/en/, "")
    .replace(/\/_index\.md$/, "/")
    .replace(/\.md$/, "/");
}

// ---------------------------------------------------------------------------
// Discover pinned pages
// ---------------------------------------------------------------------------

function discoverShortcodePages(): Map<string, string> {
  // Returns shortcode → URL
  const shortcodesDir = path.join(SITE_ROOT, "layouts", "shortcodes");
  const contentDir = path.join(SITE_ROOT, "content", "en");

  const result = new Map<string, string>();

  if (!fs.existsSync(shortcodesDir)) return result;

  const shortcodes = fs
    .readdirSync(shortcodesDir)
    .filter((f) => f.endsWith(".html"))
    .map((f) => f.replace(/\.html$/, ""));

  for (const sc of shortcodes) {
    // Grep for usage: {{< sc ... >}} or {{< sc/... >}}
    try {
      const raw = execSync(
        `grep -rl '{{[<%][ ]*${sc}[ /<]' "${contentDir}" 2>/dev/null`,
        { encoding: "utf8", stdio: ["pipe", "pipe", "pipe"] }
      ).trim();

      const files = raw.split("\n").filter(Boolean);
      if (files.length === 0) continue;

      // Prefer a file that isn't an _index.md so we get a real leaf page
      const file =
        files.find((f) => !f.includes("_index.md")) ?? files[0];
      const url = contentPathToUrl(path.relative(SITE_ROOT, file));
      result.set(sc, url);
    } catch {
      // shortcode exists but isn't used in content — skip
    }
  }

  return result;
}

function discoverLayoutPages(): Map<string, string> {
  // Returns layout-type → URL for top-level sections not covered by shortcodes
  const contentEn = path.join(SITE_ROOT, "content", "en");
  const extras: [string, string][] = [
    ["home", "/"],
    ["download", "/download/"],
    ["community", "/community/"],
    ["components", "/components/"],
  ];

  const result = new Map<string, string>();

  // Top-level content sections
  for (const section of fs.readdirSync(contentEn)) {
    const dir = path.join(contentEn, section);
    if (!fs.statSync(dir).isDirectory()) continue;
    // Pick the first leaf page (non-_index) in the section
    try {
      const raw = execSync(
        `find "${dir}" -name "*.md" ! -name "_index.md" | sort | head -1`,
        { encoding: "utf8", stdio: ["pipe", "pipe", "pipe"] }
      ).trim();
      if (raw) {
        result.set(section, contentPathToUrl(path.relative(SITE_ROOT, raw)));
      }
    } catch {
      // ignore
    }
  }

  for (const [k, v] of extras) result.set(k, v);

  return result;
}

// ---------------------------------------------------------------------------
// HTTP helpers
// ---------------------------------------------------------------------------

async function fetchText(url: string, retries = 2): Promise<{ status: number; body: string }> {
  for (let attempt = 0; attempt <= retries; attempt++) {
    try {
      const res = await fetch(url, {
        headers: { "User-Agent": "vector-compare-pages/1.0" },
        signal: AbortSignal.timeout(15_000),
      });
      const body = await res.text();
      return { status: res.status, body };
    } catch {
      if (attempt === retries) return { status: 0, body: "" };
      await new Promise((r) => setTimeout(r, 500));
    }
  }
  return { status: 0, body: "" };
}

function parseSitemapUrls(xml: string): string[] {
  const matches = xml.match(/<loc>([^<]+)<\/loc>/g) ?? [];
  return matches
    .map((m) => m.replace(/<\/?loc>/g, "").trim())
    .filter((u) => u.startsWith("http"));
}

// ---------------------------------------------------------------------------
// HTML analysis
// ---------------------------------------------------------------------------

function extractClasses(html: string): Set<string> {
  const $ = cheerio.load(html);
  const classes = new Set<string>();
  $("[class]").each((_, el) => {
    const raw = $(el).attr("class") ?? "";
    for (const cls of raw.split(/\s+/)) {
      if (cls) classes.add(cls);
    }
  });
  return classes;
}

function extractTitle(html: string): string {
  const $ = cheerio.load(html);
  return $("title").first().text().trim();
}

function extractH1(html: string): string {
  const $ = cheerio.load(html);
  return $("h1").first().text().trim();
}

function extractHeadings(html: string): string[] {
  const $ = cheerio.load(html);
  const headings: string[] = [];
  $("h1, h2, h3").each((_, el) => {
    const text = $(el).text().replace(/\s+/g, " ").trim().slice(0, 60);
    headings.push(`${el.tagName}: ${text}`);
  });
  return headings;
}

const TAILWIND_RE =
  /^(sm:|md:|lg:|xl:|2xl:|dark:|hover:|focus:|group-|active:|disabled:)/.test.bind(
    /^(sm:|md:|lg:|xl:|2xl:|dark:|hover:|focus:|group-|active:|disabled:)/
  ) ||
  /^(flex|grid|block|inline|hidden|text-|bg-|border-|p-|m-|px-|py-|mx-|my-|pt-|pb-|pl-|pr-|mt-|mb-|ml-|mr-|w-|h-|max-|min-|gap-|space-|rounded|shadow|font-|leading-|tracking-|opacity-|z-|overflow-|cursor-|transition|transform|scale-|rotate-|translate-|col-|row-|self-|items-|justify-|content-)/.test.bind(
    /^(flex|grid|block|inline|hidden|text-|bg-|border-|p-|m-|px-|py-|mx-|my-|pt-|pb-|pl-|pr-|mt-|mb-|ml-|mr-|w-|h-|max-|min-|gap-|space-|rounded|shadow|font-|leading-|tracking-|opacity-|z-|overflow-|cursor-|transition|transform|scale-|rotate-|translate-|col-|row-|self-|items-|justify-|content-)/
  );

function isTailwindClass(cls: string): boolean {
  return (
    /^(sm:|md:|lg:|xl:|2xl:|dark:|hover:|focus:|group-|active:|disabled:)/.test(cls) ||
    /^(flex|grid|block|inline|hidden|text-|bg-|border-|p[xytrblse]?-|m[xytrblse]?-|w-|h-|max-[wh]|min-[wh]|gap-|space-[xy]|rounded|shadow|font-|leading-|tracking-|opacity-|z-|overflow-|cursor-|transition|transform|scale-|rotate-|translate-[xy]|col-|row-|self-|items-|justify-|content-)/.test(cls)
  );
}

// ---------------------------------------------------------------------------
// Known v4 migration differences — expected changes, not regressions
// ---------------------------------------------------------------------------

// Classes that were renamed between v3 and v4.
const V4_RENAMED: Record<string, string> = {
  "dark:prose-dark": "dark:prose-invert",
};

// Patterns for utilities removed in v4 (replaced by color/opacity modifier syntax).
const V4_REMOVED: Array<{ pattern: RegExp; note: string }> = [
  { pattern: /^(dark:)?bg-opacity-\d+$/, note: "removed in v4 — use bg-{color}/{opacity}" },
  { pattern: /^(dark:)?text-opacity-\d+$/, note: "removed in v4 — use text-{color}/{opacity}" },
  { pattern: /^(dark:)?border-opacity-\d+$/, note: "removed in v4 — use border-{color}/{opacity}" },
  { pattern: /^(dark:)?ring-opacity-\d+$/, note: "removed in v4 — use ring-{color}/{opacity}" },
  { pattern: /^(dark:)?placeholder-opacity-\d+$/, note: "removed in v4 — use placeholder-{color}/{opacity}" },
];

function v4MigrationNote(cls: string): string | null {
  if (V4_RENAMED[cls]) return `renamed to ${V4_RENAMED[cls]}`;
  for (const { pattern, note } of V4_REMOVED) {
    if (pattern.test(cls)) return note;
  }
  return null;
}

// ---------------------------------------------------------------------------
// Report types
// ---------------------------------------------------------------------------

interface PageResult {
  path: string;
  label: string; // e.g. "shortcode:tabs", "layout:blog", "random"
  issues: string[];
  v4diffs: string[];
  classesMissingLocally: string[];
  classesOnlyLocal: string[];
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main() {
  console.log(`\n🔍 vector.dev page comparison`);
  console.log(`   prod:   ${PROD_BASE}`);
  console.log(`   local:  ${LOCAL_BASE}`);
  console.log(`   seed:   ${SEED}\n`);

  // 1. Discover pinned pages
  process.stdout.write("Discovering shortcode pages… ");
  const shortcodePages = discoverShortcodePages();
  console.log(`${shortcodePages.size} shortcodes`);

  process.stdout.write("Discovering layout pages… ");
  const layoutPages = discoverLayoutPages();
  console.log(`${layoutPages.size} layout types`);

  // 2. Fetch both CSS files — used to detect classes that have rules in prod but not locally.
  // This catches dynamically-constructed class names (e.g. `text-{{ $color }}`) that JIT
  // misses when scanning template files statically.
  process.stdout.write("Fetching CSS files… ");
  const { body: localCssBody } = await fetchText(`${LOCAL_BASE}/css/style.css`);
  const localCssText = localCssBody.replace(/\\/g, "");

  // Prod CSS is hash-named; find the URL from the prod home page HTML.
  const { body: prodHomeHtml } = await fetchText(`${PROD_BASE}/`);
  const prodCssPath = prodHomeHtml.match(/\/css\/style\.[a-f0-9]+\.css/)?.[0] ?? null;
  let prodCssText = "";
  if (prodCssPath) {
    const { body } = await fetchText(`${PROD_BASE}${prodCssPath}`);
    prodCssText = body.replace(/\\/g, "");
  }

  function classInCss(cls: string, css: string): boolean {
    const bare = cls.replace(/\\/g, "");
    return css.includes(bare);
  }
  console.log(`local ${Math.round(localCssBody.length / 1024)} KB  prod ${Math.round(prodCssText.length / 1024)} KB`);

  // 3. Fetch sitemap
  process.stdout.write("Fetching sitemap… ");
  const { body: sitemapXml, status: sitemapStatus } = await fetchText(`${PROD_BASE}/sitemap.xml`);
  if (sitemapStatus !== 200) {
    console.error(`Failed to fetch sitemap (HTTP ${sitemapStatus})`);
    process.exit(1);
  }
  const sitemapPaths = parseSitemapUrls(sitemapXml).map((u) => new URL(u).pathname);
  console.log(`${sitemapPaths.length} sitemap URLs`);

  // 3. Build ordered page list: pinned first, then random fill
  const labeled: Array<{ path: string; label: string }> = [];
  const seen = new Set<string>();

  const add = (p: string, label: string) => {
    const norm = p.endsWith("/") ? p : `${p}/`;
    if (!seen.has(norm)) {
      seen.add(norm);
      labeled.push({ path: norm, label });
    }
  };

  for (const [sc, url] of shortcodePages) add(url, `shortcode:${sc}`);
  for (const [layout, url] of layoutPages) add(url, `layout:${layout}`);

  // Add all remaining sitemap pages (shuffled for variety), up to SAMPLE_SIZE cap
  const rng = seededRandom(SEED);
  for (const p of shuffle(sitemapPaths, rng)) {
    if (labeled.length >= SAMPLE_SIZE) break;
    add(p, "random");
  }

  const pinnedCount = shortcodePages.size + layoutPages.size;
  const randomCount = labeled.length - pinnedCount;
  console.log(
    `\nTotal pages: ${labeled.length}  (${pinnedCount} pinned + ${randomCount} from sitemap)  concurrency: ${CONCURRENCY}\n`
  );

  // 4. Compare (concurrent pool)
  const results: PageResult[] = new Array(labeled.length);
  let passed = 0;
  let failed = 0;
  let completed = 0;
  const labelWidth = Math.max(...labeled.map((l) => l.label.length));
  const indexWidth = String(labeled.length).length;

  async function comparePage(i: number): Promise<void> {
    const { path, label } = labeled[i];

    const [prod, local] = await Promise.all([
      fetchText(`${PROD_BASE}${path}`),
      fetchText(`${LOCAL_BASE}${path}`),
    ]);

    const issues: string[] = [];

    if (prod.status !== 200) issues.push(`prod HTTP ${prod.status}`);
    if (local.status !== 200) issues.push(`local HTTP ${local.status}`);

    const prodClasses = extractClasses(prod.body);
    const localClasses = extractClasses(local.body);

    const allMissingLocally = [...prodClasses].filter(
      (c) => !localClasses.has(c) && isTailwindClass(c)
    );
    const onlyLocal = [...localClasses].filter(
      (c) => !prodClasses.has(c) && isTailwindClass(c)
    );

    // Split into true regressions vs known v4 migration differences.
    const missingLocally = allMissingLocally.filter((c) => !v4MigrationNote(c));
    const missingLocallyV4 = allMissingLocally.filter((c) => v4MigrationNote(c));

    if (missingLocally.length > 0) {
      issues.push(`${missingLocally.length} Tailwind class(es) missing locally`);
    }

    // Check for classes that have rules in prod CSS but are missing from local CSS.
    // This catches dynamically-constructed class names (e.g. `text-[color]`) that JIT
    // misses when scanning template files statically, while ignoring custom classes
    // defined outside Tailwind (SASS, etc.) which won't appear in either CSS file.
    const allWithoutRules = [...localClasses].filter(
      (c) => isTailwindClass(c) && classInCss(c, prodCssText) && !classInCss(c, localCssText)
    );
    const classesWithoutRules = allWithoutRules.filter((c) => !v4MigrationNote(c));
    const classesWithoutRulesV4 = allWithoutRules.filter((c) => v4MigrationNote(c));

    if (classesWithoutRules.length > 0) {
      issues.push(`${classesWithoutRules.length} class(es) in prod CSS but missing from local CSS`);
    }

    // Collect all distinct v4 migration notes for this page.
    const v4diffs = [...new Set([...missingLocallyV4, ...classesWithoutRulesV4].map(
      (c) => `${c} → ${v4MigrationNote(c)}`
    ))];

    const prodTitle = extractTitle(prod.body);
    const localTitle = extractTitle(local.body);
    if (prodTitle !== localTitle && prod.status === 200 && local.status === 200) {
      issues.push(`title mismatch`);
    }

    const prodH1 = extractH1(prod.body);
    const localH1 = extractH1(local.body);
    if (prodH1 !== localH1 && prod.status === 200 && local.status === 200) {
      issues.push(`h1 mismatch`);
    }

    const prodHeadings = extractHeadings(prod.body);
    const localHeadings = extractHeadings(local.body);
    if (
      JSON.stringify(prodHeadings) !== JSON.stringify(localHeadings) &&
      prod.status === 200 &&
      local.status === 200
    ) {
      issues.push(`heading structure differs`);
    }

    const ok = issues.length === 0;
    const hasV4Diffs = v4diffs.length > 0;
    completed++;
    if (ok) passed++;
    else failed++;

    const prefix = `[${String(completed).padStart(indexWidth)}/${labeled.length}] ${label.padEnd(labelWidth)}  ${path}`;
    if (ok && !hasV4Diffs) {
      console.log(`${prefix} ✓`);
    } else if (ok && hasV4Diffs) {
      console.log(`${prefix} ✓  (v4 diff)`);
    } else {
      console.log(`${prefix} ✗  [${issues.join(", ")}]`);
    }

    if (missingLocally.length > 0) {
      console.log(
        `      Missing: ${missingLocally.slice(0, 8).join("  ")}${missingLocally.length > 8 ? ` …+${missingLocally.length - 8}` : ""}`
      );
    }
    if (hasV4Diffs) {
      for (const note of v4diffs.slice(0, 4)) {
        console.log(`      v4 diff: ${note}`);
      }
      if (v4diffs.length > 4) console.log(`      v4 diff: …+${v4diffs.length - 4} more`);
    }
    if (VERBOSE && onlyLocal.length > 0) {
      console.log(`      Only local (${onlyLocal.length}): ${onlyLocal.slice(0, 6).join("  ")}`);
    }
    if (classesWithoutRules.length > 0) {
      console.log(
        `      Missing rule: ${classesWithoutRules.slice(0, 8).join("  ")}${classesWithoutRules.length > 8 ? ` …+${classesWithoutRules.length - 8}` : ""}`
      );
    }

    results[i] = { path, label, issues, v4diffs, classesMissingLocally: missingLocally, classesOnlyLocal: onlyLocal };
  }

  // Run with bounded concurrency
  const queue = labeled.map((_, i) => i);
  const workers = Array.from({ length: Math.min(CONCURRENCY, labeled.length) }, async () => {
    while (queue.length > 0) {
      const i = queue.shift()!;
      await comparePage(i);
    }
  });
  await Promise.all(workers);

  // 5. Summary
  const v4DiffPages = results.filter((r) => r.issues.length === 0 && r.v4diffs.length > 0);
  console.log(`\n${"─".repeat(70)}`);
  console.log(`Results: ${passed}/${labeled.length} passed, ${failed} failed, ${v4DiffPages.length} with v4 diffs`);

  if (failed > 0) {
    console.log(`\nFailed pages:`);
    for (const r of results.filter((r) => r.issues.length > 0)) {
      console.log(`  [${r.label}]  ${r.path}`);
      for (const issue of r.issues) console.log(`    · ${issue}`);
      if (r.classesMissingLocally.length > 0) {
        console.log(`    · Missing: ${r.classesMissingLocally.join(", ")}`);
      }
    }
  }

  if (v4DiffPages.length > 0) {
    // Summarise the distinct v4 diffs across all pages (not per-page, to avoid noise).
    const allV4Notes = new Map<string, number>();
    for (const r of v4DiffPages) {
      for (const note of r.v4diffs) {
        allV4Notes.set(note, (allV4Notes.get(note) ?? 0) + 1);
      }
    }
    console.log(`\nKnown v4 migration diffs (expected, not regressions):`);
    for (const [note, count] of [...allV4Notes.entries()].sort((a, b) => b[1] - a[1])) {
      console.log(`  ${String(count).padStart(3)}x  ${note}`);
    }
  }

  // Global missing class frequency
  const allMissing = new Map<string, number>();
  for (const r of results) {
    for (const cls of r.classesMissingLocally) {
      allMissing.set(cls, (allMissing.get(cls) ?? 0) + 1);
    }
  }
  if (allMissing.size > 0) {
    const sorted = [...allMissing.entries()].sort((a, b) => b[1] - a[1]);
    console.log(`\nMost frequently missing Tailwind classes:`);
    for (const [cls, count] of sorted.slice(0, 20)) {
      console.log(`  ${String(count).padStart(3)}x  ${cls}`);
    }
  }

  console.log();
  process.exit(failed > 0 ? 1 : 0);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
