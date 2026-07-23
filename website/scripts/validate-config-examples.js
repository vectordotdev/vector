import fs from "fs";
import os from "os";
import path from "path";
import { execSync } from "child_process";
import chalk from "chalk";
import YAML from "yaml";

const cueJsonOutput = "data/docs.json";
const allowlistPath = new URL("./validate-config-examples-allowlist.json", import.meta.url).pathname;
const VECTOR_BIN = process.env.VECTOR_BIN || "vector";

// Pick a source type compatible with the component's accepted input event types.
// Returns null for trace-only components (no simple trace source available).
const sourceTypeFor = (component) => {
  const input = component.input;
  if (!input || input.logs) return "demo_logs";
  if (input.metrics) return "internal_metrics";
  return null; // trace-only — skip validation
};

// Wrap a component's example YAML in a complete topology so vector validate accepts it.
// Returns null when the component cannot be wrapped (e.g. trace-only).
const wrapConfig = (kind, componentYaml, component) => {
  const parsed = YAML.parse(componentYaml);

  if (kind === "sources") {
    const sourceKey = Object.keys(parsed.sources)[0];
    return YAML.stringify({
      ...parsed,
      sinks: {
        _validate_sink: {
          type: "blackhole",
          inputs: [sourceKey]
        }
      }
    });
  }

  const sourceType = sourceTypeFor(component);
  if (sourceType === null) return null;
  const validateSource = sourceType === "demo_logs" ? { type: "demo_logs", format: "json" } : { type: sourceType };

  if (kind === "transforms") {
    const transformKey = Object.keys(parsed.transforms)[0];
    return YAML.stringify({
      sources: { _validate_source: validateSource },
      transforms: {
        [transformKey]: {
          ...parsed.transforms[transformKey],
          inputs: ["_validate_source"]
        }
      },
      sinks: {
        _validate_sink: {
          type: "blackhole",
          inputs: [transformKey]
        }
      }
    });
  }

  if (kind === "sinks") {
    const sinkKey = Object.keys(parsed.sinks)[0];
    return YAML.stringify({
      sources: { _validate_source: validateSource },
      sinks: {
        [sinkKey]: {
          ...parsed.sinks[sinkKey],
          inputs: ["_validate_source"]
        }
      }
    });
  }

  return componentYaml;
};

const validateYaml = (yaml, tmpPath) => {
  fs.writeFileSync(tmpPath, yaml, "utf8");
  try {
    execSync(`${VECTOR_BIN} validate --no-environment --skip-healthchecks ${tmpPath}`, {
      stdio: "pipe"
    });
    return null;
  } catch (err) {
    return (err.stderr?.toString() || err.stdout?.toString() || err.message).trim();
  }
};

const summarizeError = (error) => {
  const lines = error.split("\n").filter((l) => l.trim());
  const errorLine = lines.find(
    (l) => !l.includes("Failed to load") && !l.includes("-----") && !l.startsWith("error[") && l.includes("x ")
  );
  return (errorLine || lines[0] || error).trim().replace(/^x /, "");
};

const main = () => {
  const allowlist = new Set(JSON.parse(fs.readFileSync(allowlistPath, "utf8")));

  const data = fs.readFileSync(cueJsonOutput, "utf8");
  const docs = JSON.parse(data);
  const components = docs.components;

  const newFailures = [];
  const knownFailures = [];
  let total = 0;
  let skipped = 0;
  const tmpFile = path.join(os.tmpdir(), "vector-validate-example.yaml");

  try {
    for (const kind in components) {
      for (const componentType in components[kind]) {
        const component = components[kind][componentType];
        const exampleConfigs = component.example_configs;
        if (!exampleConfigs) continue;

        for (const variant of ["minimal", "advanced"]) {
          const yaml = exampleConfigs[variant]?.yaml;
          if (!yaml) continue;

          total++;
          const key = `${kind}/${componentType} (${variant})`;

          let wrapped;
          try {
            wrapped = wrapConfig(kind, yaml, component);
          } catch (e) {
            const error = `YAML parse error: ${e.message}`;
            if (allowlist.has(key)) {
              knownFailures.push({ key, error });
            } else {
              newFailures.push({ key, error });
              console.error(chalk.red(`NEW FAIL ${key} [parse error]`));
            }
            continue;
          }

          if (wrapped === null) {
            skipped++;
            continue;
          }

          const error = validateYaml(wrapped, tmpFile);
          if (error) {
            const summary = summarizeError(error);
            if (allowlist.has(key)) {
              knownFailures.push({ key, error: summary });
            } else {
              newFailures.push({ key, error: summary });
              console.error(chalk.red(`NEW FAIL ${key}: ${summary}`));
            }
          }
        }
      }
    }
  } finally {
    if (fs.existsSync(tmpFile)) fs.unlinkSync(tmpFile);
  }

  const validated = total - skipped;
  console.log(
    chalk.gray(`Validated ${validated} examples (${skipped} skipped, ${knownFailures.length} known failures).`)
  );

  if (newFailures.length === 0) {
    console.log(chalk.green("No new validation failures."));
  } else {
    console.error(
      chalk.red(`\n${newFailures.length} new validation failure(s) — update the allowlist if intentional.`)
    );
    process.exit(1);
  }
};

main();
