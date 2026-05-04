import tailwindcss from "@tailwindcss/postcss";
import { readFileSync, writeFileSync, existsSync } from "fs";
import { join, dirname } from "path";
import { fileURLToPath } from "url";

// Generate .hugo-classes from hugo_stats.json so Tailwind can detect dynamically-
// constructed class names (e.g. `text-{{ $color }}`) that static file scanning misses.
// This file is written at PostCSS startup time, before style.css is processed.
const ROOT = dirname(fileURLToPath(import.meta.url));
const statsPath = join(ROOT, "hugo_stats.json");
const classesPath = join(ROOT, ".hugo-classes");

try {
  if (existsSync(statsPath)) {
    const { htmlElements: { classes = [] } = {} } = JSON.parse(readFileSync(statsPath, "utf8"));
    writeFileSync(classesPath, classes.join("\n"));
  } else {
    writeFileSync(classesPath, "");
  }
} catch {
  writeFileSync(classesPath, "");
}

export default {
  plugins: [tailwindcss],
};
