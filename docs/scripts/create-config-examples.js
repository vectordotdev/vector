const fs = require('fs');
const cueJsonOutput = "data/docs.json";
const chalk = require('chalk');
const TOML = require('@iarna/toml');
const YAML = require('yaml');

const getExampleValue = (exampleConfig, paramName, param) => {
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
  var required = {};

  for (const paramName in configuration) {
    if (paramName != "type") {
      const param = configuration[paramName];

      getExampleValue(required, paramName, param);
    }
  }

  return required;
}

const makeOptionalParams = (configuration) => {
  var optional = {};

  for (const paramName in configuration) {
    if (paramName != "type") {
      const param = configuration[paramName];

      getExampleValue(optional, paramName, param);
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
      const advancedParams = makeOptionalParams(configuration);

      const keyName = `my_${kind.substring(0, kind.length - 1)}_id`;

      var common = null,
        advanced = null;

      if (['sinks', 'transforms'].includes(kind)) {
        common = {
          [kind]: {
            [keyName]: {
              "type": componentType,
              inputs: ['my-source-or-transform-id'], // Sinks and transforms need this
              ...commonParams,
            }
          }
        };

        advanced = {
          [kind]: {
            [keyName]: {
              "type": componentType,
              inputs: ['my-source-or-transform-id'],
              ...commonParams,
              ...advancedParams,
            }
          }
        };
      } else {
        common = {
          [kind]: {
            [keyName]: {
              "type": componentType,
              ...commonParams,
            }
          }
        };

        advanced = {
          [kind]: {
            [keyName]: {
              "type": componentType,
              ...commonParams,
              ...advancedParams,
            }
          }
        };
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
