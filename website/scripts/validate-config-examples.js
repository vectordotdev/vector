import fs from "fs";
import os from "os";
import path from "path";
import { execFile } from "child_process";
import { promisify } from "util";
import chalk from "chalk";
import YAML from "yaml";

const cueJsonOutput = "data/docs.json";
const generatedExamplesDir = "generated/example-configs";
const VECTOR_BIN = process.env.VECTOR_BIN || "vector";
const validationConcurrency = Math.max(1, Number.parseInt(process.env.VALIDATE_CONFIG_EXAMPLES_JOBS || "4", 10) || 4);
const execFileAsync = promisify(execFile);

// Use `cargo run` when running inside the Vector repo and no specific binary is needed.
// Set VECTOR_BIN to point at an existing binary and skip the cargo build.
const useCargoRun = !process.env.VECTOR_BIN && fs.existsSync(path.join(import.meta.dirname, "../../Cargo.toml"));
const vectorCommand = useCargoRun ? "cargo" : VECTOR_BIN;

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
    const outputs = component.outputs || [];
    const hasNamedOutputs = outputs.length > 0 && outputs[0].name !== "<component_id>";
    const sinks = {};
    if (hasNamedOutputs) {
      outputs.forEach(({ name }) => {
        sinks[`_validate_sink_${name}`] = { type: "blackhole", inputs: [`${sourceKey}.${name}`] };
      });
    } else {
      sinks["_validate_sink"] = { type: "blackhole", inputs: [sourceKey] };
    }
    return YAML.stringify({ ...parsed, sinks });
  }

  const sourceType = sourceTypeFor(component);
  if (sourceType === null) return null;
  const validateSource = sourceType === "demo_logs" ? { type: "demo_logs", format: "json" } : { type: sourceType };

  if (kind === "transforms") {
    const transformKey = Object.keys(parsed.transforms)[0];
    const transformConfig = { ...parsed.transforms[transformKey], inputs: ["_validate_source"] };

    // route uses a map of route-id → condition; exclusive_route uses an array of {name, condition}
    const namedOutputs = [
      ...Object.keys(transformConfig.route ?? {}),
      ...(transformConfig.routes ?? []).map((r) => r?.name).filter(Boolean)
    ];

    const blackhole = (input) => ({ type: "blackhole", inputs: [input] });
    const sinks =
      namedOutputs.length > 0
        ? Object.fromEntries(namedOutputs.map((n) => [`_validate_sink_${n}`, blackhole(`${transformKey}.${n}`)]))
        : { _validate_sink: blackhole(transformKey) };

    return YAML.stringify({
      sources: { _validate_source: validateSource },
      transforms: { [transformKey]: transformConfig },
      sinks
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

const validateYaml = async (yaml, tmpPath) => {
  fs.writeFileSync(tmpPath, yaml, "utf8");
  try {
    const args = ["validate", "--no-environment", "--skip-healthchecks", tmpPath];
    if (useCargoRun) args.unshift("run", "--");

    await execFileAsync(vectorCommand, args, {
      maxBuffer: 1024 * 1024
    });
    return null;
  } catch (err) {
    return (err.stderr?.toString() || err.stdout?.toString() || err.message).trim();
  } finally {
    if (fs.existsSync(tmpPath)) fs.unlinkSync(tmpPath);
  }
};

const summarizeError = (error) => {
  const lines = error.split("\n").filter((l) => l.trim());
  const errorLine = lines.find(
    (l) => !l.includes("Failed to load") && !l.includes("-----") && !l.startsWith("error[") && l.includes("x ")
  );
  return (errorLine || lines[0] || error).trim().replace(/^x /, "");
};

const main = async () => {
  const data = fs.readFileSync(cueJsonOutput, "utf8");
  const docs = JSON.parse(data);
  const components = docs.components;

  const failures = [];
  const validationCases = [];
  let total = 0;
  let skipped = 0;

  for (const kind in components) {
    for (const componentType in components[kind]) {
      const component = components[kind][componentType];

      for (const variant of ["minimal", "advanced"]) {
        total++;
        const key = `${kind}/${componentType} (${variant})`;
        const examplePath = path.join(generatedExamplesDir, kind, componentType, `${variant}.yaml`);

        let yaml;
        try {
          yaml = fs.readFileSync(examplePath, "utf8");
        } catch (err) {
          failures.push({ key, error: `Could not read ${examplePath}: ${err.message}` });
          console.error(chalk.red(`FAIL ${key}: Could not read ${examplePath}`));
          continue;
        }

        let wrapped;
        try {
          wrapped = wrapConfig(kind, yaml, component);
        } catch (e) {
          failures.push({ key, error: `YAML parse error: ${e.message}` });
          console.error(chalk.red(`FAIL ${key} [parse error]`));
          continue;
        }

        if (wrapped === null) {
          skipped++;
          continue;
        }

        validationCases.push({ key, wrapped });
      }
    }
  }

  let nextCase = 0;
  const validateNext = async () => {
    while (nextCase < validationCases.length) {
      const caseIndex = nextCase++;
      const { key, wrapped } = validationCases[caseIndex];
      const tmpFile = path.join(os.tmpdir(), `vector-validate-example-${process.pid}-${caseIndex}.yaml`);
      const error = await validateYaml(wrapped, tmpFile);

      if (error) {
        const summary = summarizeError(error);
        failures.push({ key, error: summary });
        console.error(chalk.red(`FAIL ${key}: ${summary}`));
        console.error(chalk.gray("--- config ---\n" + wrapped + "---"));
      }
    }
  };

  await Promise.all(Array.from({ length: Math.min(validationConcurrency, validationCases.length) }, validateNext));

  const validated = total - skipped;
  console.log(chalk.gray(`Validated ${validated} examples (${skipped} skipped).`));

  if (failures.length === 0) {
    console.log(chalk.green("All examples passed."));
  } else {
    console.error(chalk.red(`\n${failures.length} validation failure(s).`));
    process.exit(1);
  }
};

main().catch((err) => {
  console.error(err);
  process.exitCode = 1;
});
