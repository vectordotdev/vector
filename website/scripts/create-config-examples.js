const fs = require('fs');
const cueJsonOutput = "data/docs.json";
const chalk = require('chalk');
const TOML = require('@iarna/toml');
const YAML = require('yaml');

// Helper functions
const getExampleValue = (param, deepFilter) => {
  let value;

  const getArrayValue = (obj) => {
    const enumVal = (obj.enum != null) ? [Object.keys(obj.enum)[0]] : null;

    const examplesVal = (obj.examples != null && obj.examples.length > 0) ? [obj.examples[0]] : null;

    return obj.default || examplesVal || enumVal || null;
  }

  const getValue = (obj) => {
    const enumVal = (obj.enum != null) ? Object.keys(obj.enum)[0] : null;

    const examplesVal = (obj.examples != null && obj.examples.length > 0) ? obj.examples[0] : null;

    return obj.default || examplesVal || enumVal || null;
  }

  Object.keys(param.type).forEach(k => {
    const p = param.type[k];

    if (['array', 'object'].includes(k)) {
      const topType = k;

      if (p.items && p.items.type) {
        const typeInfo = p.items.type;

        Object.keys(typeInfo).forEach(k => {
          if (['array', 'object'].includes(k)) {
            const subType = k;
            const options = typeInfo[k].options;

            var subObj = {};

            Object
              .keys(options)
              .filter(k => deepFilter(options[k]))
              .forEach(k => {
                Object.keys(options[k].type).forEach(key => {
                  const deepTypeInfo = options[k].type[key];

                  if (subType === 'array') {
                    subObj[k] = getArrayValue(deepTypeInfo);
                  } else {
                    subObj[k] = getValue(deepTypeInfo);
                  }

                });
              });

            value = subObj;
          } else {
            if (topType === 'array') {
              value = getArrayValue(typeInfo[k]);
            } else {
              value = getValue(typeInfo[k]);
            }
          }
        });
      } else {
        value = getValue(p);
      }
    } else {
      value = getValue(p);
    }
  });

  return value;
}

Object.makeExampleParams = (params, filter, deepFilter) => {
  var obj = {};

  Object
    .keys(params)
    .filter(k => filter(params[k]))
    .forEach(k => {
      let value = getExampleValue(params[k], deepFilter);
      if (value) {
        obj[k] = value;
      }
    });

  return obj;
}

// Convert object to TOML string
const toToml = (obj) => {
  return TOML.stringify(obj);
}

// Convert object to YAML string
const toYaml = (obj) => {
  return `${YAML.stringify(obj)}`;
}

// Convert object to JSON string (indented)
const toJson = (obj) => {
  return JSON.stringify(obj, null, 2);
}

// Set the example value for a given config parameter
const setExampleValue = (exampleConfig, paramName, param) => {
  // Because the `type` field can have one of several different values
  // (`string`, `array`, `object`, etc.) you need to use recursion here to
  // get through to the lower level params, e.g. `type.string.examples`. If
  // there's a more idiomatic way to do this in JS, please advise.
  Object.keys(param.type).forEach((k) => {
    const p = param.type[k];

    if (p.default) {
      exampleConfig[paramName] = p.default;
    }

    if (p.examples != null && p.examples.length > 0) {
      exampleConfig[paramName] = p.examples[0];
    }

    if (['array', 'object'].includes(k)) {
      if (p.items) {
        var obj = {};

        Object.keys(p.items.type).forEach((t) => {
          const typeInfo = p.items.type[t];

          if (typeInfo.examples && typeInfo.examples.length > 0) {
            exampleConfig[paramName] = typeInfo.examples[0];
          }

          if (typeInfo.options) {
            Object.keys(typeInfo.options).forEach((k) => {
              const opt = typeInfo.options[k];

              if (opt.required) {
                Object.keys(opt.type).forEach((t) => {
                  const typeInfo = opt.type[t];

                  if (typeInfo.examples && typeInfo.examples.length > 0) {
                    obj[k] = typeInfo.examples[0];
                  }
                });
              }
            });

            exampleConfig[paramName] = obj;
          }
        });
      }
    }
  });
}

// Assemble the "common" params for an example config
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

// Assemble the "advanced" params for an example config
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

// Convert the use case examples (`component.examples`) into multi-format
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

      // Strip the "log" or "metric" key in the example output
      let output;

      if (example.output) {
        if (example.output['log']) {
          output = example.output['log'];
        } else if (example.output['metric']) {
          output = example.output['metric'];
        } else {
          output = example.output;
        }
      } else {
        output = example.output;
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
        output: output,
      }

      useCases.push(useCase);
    });

    return useCases;
  } else {
    return null;
  }
}

const main = () => {
  try {
    const debug = process.env.DEBUG === "true" || false;
    const data = fs.readFileSync(cueJsonOutput, 'utf8');
    const docs = JSON.parse(data);
    const components = docs.components;

    console.log(chalk.blue("Creating example configurations for all Vector components..."));

    // Sources, transforms, sinks
    for (const kind in components) {
      console.log(chalk.blue(`Creating examples for ${kind}...`));

      const componentsOfKind = components[kind];

      // Specific components
      for (const componentType in componentsOfKind) {
        const component = componentsOfKind[componentType];
        const configuration = component.configuration;

        const commonParams = Object.makeExampleParams(
          configuration,
          p => p.required || p.common,
          p => p.required || p.common,
        );
        const advancedParams = Object.makeExampleParams(
          configuration,
          _ => true,
          p => p.required || p.common || p.relevant_when,
        );
        const useCaseExamples = makeUseCaseExamples(component);

        const keyName = `my_${kind.substring(0, kind.length - 1)}_id`;

        let commonExampleConfig, advancedExampleConfig;

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
                ...advancedParams,
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
                ...advancedParams,
              }
            }
          };
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


    // A debugging statement to make sure things are going basically as planned
    if (debug) {
      console.log(docs['components']['sources']['syslog']['examples']);
    }


    console.log(chalk.green("Success. Finished generating examples for all components."));
    console.log(chalk.blue(`Writing generated examples as JSON to ${cueJsonOutput}...`));

    // Write back to the JSON file only when not in debug mode
    if (!debug) {
      fs.writeFileSync(cueJsonOutput, JSON.stringify(docs), 'utf8');
    }

    console.log(chalk.green(`Success. Finished writing example configs to ${cueJsonOutput}.`));
  } catch (err) {
    console.error(err);
  }
}

main();
