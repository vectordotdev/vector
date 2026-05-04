#!/usr/bin/env tsx
/**
 * Fetches a random sample of pages from production and a local dev server,
 * then compares them 1:1 for regressions after a CSS/build upgrade.
 *
 * Usage:
 *   tsx scripts/compare-pages.ts [options]
 *
 * Options:
 *   --sample N       Number of pages to sample (default: 30)
 *   --local URL      Local server base URL (default: http://localhost:1313)
 *   --prod URL       Production base URL (default: https://vector.dev)
 *   --seed N         Random seed for reproducible sampling (default: random)
 *   --verbose        Print per-class diffs even for passing pages
 */

import * as cheerio from "cheerio";

// ---------------------------------------------------------------------------
// CLI args
// ---------------------------------------------------------------------------

const args = process.argv.slice(2);
const flag = (name: string, fallback: string) => {
  const idx = args.indexOf(name);
  return idx !== -1 && args[idx + 1] ? args[idx + 1] : fallback;
};
const hasFlag = (name: string) => args.includes(name);

const SAMPLE_SIZE = parseInt(flag("--sample", "30"), 10);
const LOCAL_BASE = flag("--local", "http://localhost:1313");
const PROD_BASE = flag("--prod", "https://vector.dev");
const SEED = parseInt(flag("--seed", String(Date.now())), 10);
const VERBOSE = hasFlag("--verbose");

// ---------------------------------------------------------------------------
// Seeded RNG (mulberry32) for reproducible sampling
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

function sample<T>(arr: T[], n: number, rng: () => number): T[] {
  const copy = [...arr];
  for (let i = copy.length - 1; i > 0; i--) {
    const j = Math.floor(rng() * (i + 1));
    [copy[i], copy[j]] = [copy[j], copy[i]];
  }
  return copy.slice(0, n);
}

// ---------------------------------------------------------------------------
// Helpers
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
    } catch (err) {
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
    headings.push(`${el.tagName}: ${$(el).text().trim().slice(0, 60)}`);
  });
  return headings;
}

// ---------------------------------------------------------------------------
// Report types
// ---------------------------------------------------------------------------

interface PageResult {
  path: string;
  localStatus: number;
  prodStatus: number;
  titleMatch: boolean;
  localTitle: string;
  prodTitle: string;
  h1Match: boolean;
  localH1: string;
  prodH1: string;
  classesMissingLocally: string[];   // in prod but not local
  classesOnlyLocal: string[];        // in local but not prod
  headingMatch: boolean;
  issues: string[];
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main() {
  console.log(`\n🔍 vector.dev page comparison`);
  console.log(`   prod:   ${PROD_BASE}`);
  console.log(`   local:  ${LOCAL_BASE}`);
  console.log(`   sample: ${SAMPLE_SIZE} pages  (seed: ${SEED})\n`);

  // 1. Fetch sitemap
  process.stdout.write("Fetching sitemap… ");
  const { body: sitemapXml, status: sitemapStatus } = await fetchText(`${PROD_BASE}/sitemap.xml`);
  if (sitemapStatus !== 200) {
    console.error(`Failed to fetch sitemap (HTTP ${sitemapStatus})`);
    process.exit(1);
  }
  const allUrls = parseSitemapUrls(sitemapXml);
  console.log(`${allUrls.length} URLs found`);

  // 2. Sample
  const rng = seededRandom(SEED);
  const paths = sample(allUrls, Math.min(SAMPLE_SIZE, allUrls.length), rng).map(
    (u) => new URL(u).pathname
  );

  // 3. Compare each page
  const results: PageResult[] = [];
  let passed = 0;
  let failed = 0;

  for (let i = 0; i < paths.length; i++) {
    const path = paths[i];
    process.stdout.write(`[${String(i + 1).padStart(2)}/${paths.length}] ${path} … `);

    const [prod, local] = await Promise.all([
      fetchText(`${PROD_BASE}${path}`),
      fetchText(`${LOCAL_BASE}${path}`),
    ]);

    const issues: string[] = [];

    if (prod.status !== 200) issues.push(`prod HTTP ${prod.status}`);
    if (local.status !== 200) issues.push(`local HTTP ${local.status}`);

    const prodClasses = extractClasses(prod.body);
    const localClasses = extractClasses(local.body);

    // Classes that exist in prod but are missing locally — potential regression
    const missingLocally = [...prodClasses].filter((c) => !localClasses.has(c));
    // Only flag Tailwind-style classes (contain - or : or are known patterns)
    const tailwindMissing = missingLocally.filter(
      (c) =>
        /^(sm:|md:|lg:|xl:|2xl:|dark:|hover:|focus:|group-)/.test(c) ||
        /^(flex|grid|block|inline|hidden|text-|bg-|border-|p-|m-|px-|py-|mx-|my-|pt-|pb-|pl-|pr-|mt-|mb-|ml-|mr-|w-|h-|max-|min-|gap-|space-|rounded|shadow|font-|leading-|tracking-|opacity-|z-|overflow-|cursor-|transition|transform|scale-|rotate-|translate-|col-|row-|self-|items-|justify-|content-)/.test(c)
    );

    const prodTitle = extractTitle(prod.body);
    const localTitle = extractTitle(local.body);
    const titleMatch = prodTitle === localTitle;
    if (!titleMatch && prod.status === 200 && local.status === 200) {
      issues.push(`title mismatch`);
    }

    const prodH1 = extractH1(prod.body);
    const localH1 = extractH1(local.body);
    const h1Match = prodH1 === localH1;
    if (!h1Match && prod.status === 200 && local.status === 200) {
      issues.push(`h1 mismatch`);
    }

    const prodHeadings = extractHeadings(prod.body);
    const localHeadings = extractHeadings(local.body);
    const headingMatch = JSON.stringify(prodHeadings) === JSON.stringify(localHeadings);
    if (!headingMatch && prod.status === 200 && local.status === 200) {
      issues.push(`heading structure differs`);
    }

    if (tailwindMissing.length > 0) {
      issues.push(`${tailwindMissing.length} Tailwind class(es) missing locally`);
    }

    const ok = issues.length === 0;
    if (ok) passed++;
    else failed++;

    console.log(ok ? "✓" : `✗  [${issues.join(", ")}]`);

    if (!ok && tailwindMissing.length > 0) {
      console.log(`      Missing: ${tailwindMissing.slice(0, 8).join("  ")}${tailwindMissing.length > 8 ? ` …+${tailwindMissing.length - 8}` : ""}`);
    }
    if (VERBOSE && ok) {
      const onlyLocal = [...localClasses].filter((c) => !prodClasses.has(c));
      if (onlyLocal.length > 0) {
        console.log(`      Only local (${onlyLocal.length}): ${onlyLocal.slice(0, 6).join("  ")}`);
      }
    }

    results.push({
      path,
      localStatus: local.status,
      prodStatus: prod.status,
      titleMatch,
      localTitle,
      prodTitle,
      h1Match,
      localH1,
      prodH1,
      classesMissingLocally: tailwindMissing,
      classesOnlyLocal: [...localClasses].filter((c) => !prodClasses.has(c)),
      headingMatch,
      issues,
    });
  }

  // 4. Summary
  console.log(`\n${"─".repeat(60)}`);
  console.log(`Results: ${passed}/${paths.length} passed, ${failed} failed`);

  if (failed > 0) {
    console.log(`\nFailed pages:`);
    for (const r of results.filter((r) => r.issues.length > 0)) {
      console.log(`  ${r.path}`);
      for (const issue of r.issues) console.log(`    · ${issue}`);
      if (r.classesMissingLocally.length > 0) {
        console.log(`    · Missing Tailwind classes: ${r.classesMissingLocally.join(", ")}`);
      }
    }
  }

  // Global class stats
  const allMissing = new Map<string, number>();
  for (const r of results) {
    for (const cls of r.classesMissingLocally) {
      allMissing.set(cls, (allMissing.get(cls) ?? 0) + 1);
    }
  }
  if (allMissing.size > 0) {
    const sorted = [...allMissing.entries()].sort((a, b) => b[1] - a[1]);
    console.log(`\nMost frequently missing Tailwind classes (across all pages):`);
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
