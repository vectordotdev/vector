const fs = require('fs');
const cueJsonOutput = "data/docs.json";
const chalk = require('chalk');
const TOML = require('@iarna/toml');
const YAML = require('yaml');

const debug = process.env.DEBUG === "true" || false;

const toToml = (obj) => {
  return TOML.stringify(obj);
}

const toYaml = (obj) => {
  return `---\n${YAML.stringify(obj)}`;
}

const toJson = (obj) => {
  return JSON.stringify(obj, null, 2);
}

const setExampleValue = (exampleConfig, paramName, param) => {
  Object.keys(param.type).forEach((k) => {
    if (param.type[k].default) {
      exampleConfig[paramName] = param.type[k].default;
    } else {
      const examples = param.type[k].examples;

      if ((examples != null) && (examples.length > 0)) {
        exampleConfig[paramName] = examples[0];
      }
    }
  });
}

const makeCommonParams = (configuration) => {
  var common = {};

  for (const paramName in configuration) {
    if (paramName != "type") {
      const param = configuration[paramName];

      // Restrict to common params only
      if (param.common || param.required) {
        setExampleValue(common, paramName, param);
      }
    }
  }

  return common;
}

const makeUseCaseExamples = (component) => {
  if (component.examples) {
    var useCases = [];
    const kind = component.kind;
    const kindPlural = `${kind}s`;
    const keyName = `my_${kind}_id`;

    component.examples.forEach((example) => {
      const config = example.configuration;
      const extra = Object.fromEntries(Object.entries(config).filter(([_, v]) => v != null));


      let exampleConfig;

      if (["transform", "sink"].includes(component.kind)) {
        exampleConfig = {
          [kindPlural]: {
            [keyName]: {
              "type": component.type,
              inputs: ['my-source-or-transform-id'],
              ...extra
            }
          }
        }
      } else {
        exampleConfig = {
          [kindPlural]: {
            [keyName]: {
              "type": component.type,
              ...extra
            }
          }
        }
      }

      useCase = {
        title: example.title,
        description: example.description,
        configuration: {
          toml: toToml(exampleConfig),
          yaml: toYaml(exampleConfig),
          json: toJson(exampleConfig),
        },
        input: example.input,
        output: example.output,
      }

      useCases.push(useCase);
    });

    return useCases;
  } else {
    return null;
  }
}

const makeAllParams = (configuration) => {
  var optional = {};

  for (const paramName in configuration) {
    if (paramName != "type") {
      const param = configuration[paramName];

      setExampleValue(optional, paramName, param);
    }
  }

  return optional;
}

try {
  console.log(chalk.blue("Creating example configurations for all Vector components..."));

  const data = fs.readFileSync(cueJsonOutput, 'utf8');
  const docs = JSON.parse(data);
  const components = docs.components;

  // Sources, transforms, sinks
  for (const kind in components) {
    console.log(chalk.blue(`Creating examples for ${kind}...`));

    const componentsOfKind = components[kind];

    // Specific components
    for (const componentType in componentsOfKind) {
      const component = componentsOfKind[componentType];
      const configuration = component.configuration;

      const commonParams = makeCommonParams(configuration);
      const allParams = makeAllParams(configuration);
      const useCaseExamples = makeUseCaseExamples(component);

      const keyName = `my_${kind.substring(0, kind.length - 1)}_id`;

      var commonExampleConfig = null,
        advancedExampleConfig = null;

      // Sinks and transforms are treated differently because they need an `inputs` field
      if (['sinks', 'transforms'].includes(kind)) {
        commonExampleConfig = {
          [kind]: {
            [keyName]: {
              "type": componentType,
              inputs: ['my-source-or-transform-id'],
              ...commonParams,
            }
          }
        };

        advancedExampleConfig = {
          [kind]: {
            [keyName]: {
              "type": componentType,
              inputs: ['my-source-or-transform-id'],
              ...allParams,
            }
          }
        };
      } else {
        commonExampleConfig = {
          [kind]: {
            [keyName]: {
              "type": componentType,
              ...commonParams,
            }
          }
        };

        advancedExampleConfig = {
          [kind]: {
            [keyName]: {
              "type": componentType,
              ...allParams,
            }
          }
        };
      }

      // A debugging statement to make sure things are going basically as planned
      if (debug) {
        const debugComponent = "aws_ec2_metadata";
        const debugKind = "transforms";

        if (componentType === debugComponent && kind === debugKind) {
          console.log(
            chalk.blue(`Printing debug JSON for the ${debugComponent} ${debugKind.substring(0, debugKind.length - 1)}...`));

          console.log(JSON.stringify(advancedExampleConfig, null, 2));
        }
      }

      docs['components'][kind][componentType]['examples'] = useCaseExamples;

      docs['components'][kind][componentType]['example_configs'] = {
        common: {
          toml: toToml(commonExampleConfig),
          yaml: toYaml(commonExampleConfig),
          json: toJson(commonExampleConfig),
        },
        advanced: {
          toml: toToml(advancedExampleConfig),
          yaml: toYaml(advancedExampleConfig),
          json: toJson(advancedExampleConfig),
        },
      };
    }
  }

  console.log(chalk.green("Success. Finished generating examples for all components."));
  console.log(chalk.blue(`Writing generated examples as JSON to ${cueJsonOutput}...`));

  fs.writeFileSync(cueJsonOutput, JSON.stringify(docs), 'utf8');

  console.log(chalk.green(`Success. Finished writing example configs to ${cueJsonOutput}.`));
} catch (err) {
  console.error(err);
}
