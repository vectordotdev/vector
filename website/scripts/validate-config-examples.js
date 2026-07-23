import fs from "fs";
import os from "os";
import path from "path";
import { execSync } from "child_process";
import chalk from "chalk";
import YAML from "yaml";

const cueJsonOutput = "data/docs.json";
const VECTOR_BIN = process.env.VECTOR_BIN || "vector";

// Pick a source type compatible with the component's accepted input event types.
// Returns null for trace-only components (no simple trace source available).
const sourceTypeFor = (component) => {
  const input = component.input;
  if (!input || input.logs) return "stdin";
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

  if (kind === "transforms") {
    const transformKey = Object.keys(parsed.transforms)[0];
    return YAML.stringify({
      sources: { _validate_source: { type: sourceType } },
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
      sources: { _validate_source: { type: sourceType } },
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
  const warnOnly = process.argv.includes("--warn-only");

  const data = fs.readFileSync(cueJsonOutput, "utf8");
  const docs = JSON.parse(data);
  const components = docs.components;

  const failures = [];
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
          let wrapped;
          try {
            wrapped = wrapConfig(kind, yaml, component);
          } catch (e) {
            failures.push({ kind, componentType, variant, error: `YAML parse error: ${e.message}` });
            console.error(chalk.red(`FAIL ${kind}/${componentType} (${variant}) [parse error]`));
            continue;
          }

          if (wrapped === null) {
            skipped++;
            continue;
          }

          const error = validateYaml(wrapped, tmpFile);
          if (error) {
            failures.push({ kind, componentType, variant, error });
            const summary = summarizeError(error);
            console.error(chalk.red(`FAIL ${kind}/${componentType} (${variant}): ${summary}`));
          }
        }
      }
    }
  } finally {
    if (fs.existsSync(tmpFile)) fs.unlinkSync(tmpFile);
  }

  const validated = total - skipped;
  if (failures.length === 0) {
    console.log(chalk.green(`All ${validated} example configs passed vector validate (${skipped} skipped).`));
  } else {
    console.error(
      chalk.yellow(`\n${failures.length}/${validated} example config(s) failed validation (${skipped} skipped).`)
    );
    if (!warnOnly) process.exit(1);
  }
};

main();
