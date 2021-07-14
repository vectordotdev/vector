const fs = require('fs');
const cueJsonOutput = "data/docs.json";
const chalk = require('chalk');
const TOML = require('@iarna/toml');
const YAML = require('yaml');

const makeRequiredParams = (configuration) => {
  var required = {};

  for (const paramName in configuration) {
    if (paramName != "type") {
      const param = configuration[paramName];
      if (param.required) {
        Object.keys(param.type).forEach((k) => {
          const examples = param.type[k].examples;

          if (examples != null && examples.length > 0) {
            required[paramName] = examples[0];
          }
        });
      }
    }
  }

  return required;
}

const makeOptionalParams = (configuration) => {
  var optional = {};

  for (const paramName in configuration) {
    if (paramName != "type") {
      const param = configuration[paramName];
      if (!param.required) {
        Object.keys(param.type).forEach((k) => {
          const examples = param.type[k].examples;

          if (examples != null && examples.length > 0) {
            optional[paramName] = examples[0];
          }
        });
      }
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

      const required = makeRequiredParams(configuration);
      const optional = makeOptionalParams(configuration);

      const keyName = `my_${kind.substring(0, kind.length - 1)}_id`;

      var common = null,
        advanced = null;

      if (['sinks', 'transforms'].includes(kind)) {
        common = {
          [kind]: {
            [keyName]: {
              "type": componentType,
              inputs: ['my-source-or-transform-id'], // Sinks and transforms need this
              ...required,
            }
          }
        };

        advanced = {
          [kind]: {
            [keyName]: {
              "type": componentType,
              inputs: ['my-source-or-transform-id'],
              ...required,
              ...optional,
            }
          }
        };
      } else {
        common = {
          [kind]: {
            [keyName]: {
              "type": componentType,
              ...required,
            }
          }
        };

        advanced = {
          [kind]: {
            [keyName]: {
              "type": componentType,
              ...required,
              ...optional,
            }
          }
        };
      }

      if (componentType === "kubernetes_logs") {
        console.log(common);
      }

      docs['components'][kind][componentType]['example_configs'] = {
        common: {
          toml: TOML.stringify(common),
          yaml: `---\n${YAML.stringify(common)}`,
          json: JSON.stringify(common, null, 2),
        },
        advanced: {
          toml: TOML.stringify(advanced),
          yaml: `---\n${YAML.stringify(advanced)}`,
          json: JSON.stringify(advanced, null, 2)
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
