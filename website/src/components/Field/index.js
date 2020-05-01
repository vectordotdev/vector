import React, { useState } from "react";
import classnames from "classnames";
import { MDXProvider } from "@mdx-js/react";
import CodeBlock from "@theme/CodeBlock";

//
// Misc
//

function isObject(a) {
  return !!a && a.constructor === Object;
}

//
// TOML
//

function keyToTOML(key) {
  if (key.includes(".")) {
    return '"' + key + '"';
  } else {
    return key;
  }
}

function valueToTOML(value) {
  if (typeof value == "string" && value.includes("\n")) {
    return `"""
${value}
"""`;
  } else {
    return JSON.stringify(value);
  }
}

function kvToTOML(name, example) {
  if (isObject(example)) {
    if ("name" in example && "value" in example) {
      return `${keyToTOML(example.name)} = ${valueToTOML(example.value)}`;
    } else {
      return `${keyToTOML(Object.keys(example)[0])} = ${valueToTOML(
        Object.values(example)[0]
      )}`;
    }
  } else if (name) {
    return `${keyToTOML(name)} = ${valueToTOML(example)}`;
  } else {
    return valueToTOML(example);
  }
}

//
// Enum
//

function Enum({ values }) {
  let elements = [];

  if (!Array.isArray(values)) {
    for (var key in values) {
      elements.push(
        <code key={key} className="with-info-icon" title={values[key]}>
          {valueToTOML(key)}
        </code>
      );
      elements.push(" ");
    }
  } else {
    for (var index in values) {
      let value = values[index];
      elements.push(<code key={value}>{valueToTOML(value)}</code>);
      elements.push(" ");
    }
  }

  return elements;
}

//
// Examples
//

function Example({ name, path, unit, value }) {
  let unitText = "";

  if (unit) {
    unitText = <> ({unit})</>;
  }

  return (
    <>
      <code>{valueToTOML(value)}</code>
      {unitText}
    </>
  );
}

function Examples({ name, path, values }) {
  let code = "";

  values.forEach(function (value) {
    if (path) {
      code += `${path}.`;
    }

    code += kvToTOML(name, value) + "\n";
  });

  return (
    <div>
      <CodeBlock className="language-toml">{code}</CodeBlock>
    </div>
  );
}

//
// Groups
//

function Groups({ values }) {
  let elements = [];

  for (var index in values) {
    let value = values[index];
    elements.push(<code key={value}>{value}</code>);
    elements.push(" ");
  }

  return elements;
}

//
// Values
//

function Value({ unit, value }) {
  let unitText = "";

  if (unit) {
    unitText = <> ({unit})</>;
  }

  return (
    <>
      <code>{valueToTOML(value)}</code>
      {unitText}
    </>
  );
}

function Values({ values }) {
  let elements = [];

  values.forEach((value) => elements.push(<Value value={value} />));

  return elements;
}

//
// Relevance
//

function RelevantWhen({ value }) {
  let relKey = Object.keys(value)[0];
  let relValue = Object.values(value)[0];

  if (relValue == "") {
    relValue = null;
  }

  return (
    <span>
      <code>
        <a href={`#${relKey}`}>{relKey}</a>
      </code>{" "}
      = <code>{valueToTOML(relValue)}</code>
    </span>
  );
}

//
// Fields
//

function FieldFooter({
  defaultValue,
  enumValues,
  examples,
  groups,
  name,
  path,
  relevantWhen,
  required,
  unit,
  warnings,
}) {
  const [showExamples, setShowExamples] = useState(false);

  return (
    <ul className="info">
      {warnings &&
        warnings.length > 0 &&
        warnings.map((warning, idx) => (
          <li key={idx} className="warning">
            <i className="feather icon-alert-triangle"></i> WARNING:{" "}
            {warning.text}
          </li>
        ))}
      {relevantWhen && (
        <li>
          Only {required ? "required" : "relevant"} when:{" "}
          <RelevantWhen value={relevantWhen} />
        </li>
      )}
      {defaultValue !== undefined ? (
        defaultValue !== null ? (
          <li>
            Default: <Value unit={unit} value={defaultValue} />
          </li>
        ) : (
          <li>No default</li>
        )
      ) : null}
      {enumValues && (
        <li>
          Enum, must be one of: <Enum values={enumValues} />
        </li>
      )}
      {(examples.length > 1 || examples[0] != defaultValue) && (
        <li>
          <div
            className="show-more"
            onClick={() => setShowExamples(!showExamples)}
          >
            {showExamples ? "Hide examples" : "View examples"}
          </div>
          {showExamples && (
            <Examples name={name} path={path} values={examples} />
          )}
        </li>
      )}
    </ul>
  );
}

function Field({
  children,
  common,
  defaultValue,
  enumValues,
  examples,
  groups,
  name,
  path,
  relevantWhen,
  required,
  templateable,
  type,
  unit,
  warnings,
}) {
  const [collapse, setCollapse] = useState(false);

  let filteredChildren = children;

  if (collapse) {
    filteredChildren = filteredChildren.filter(
      (child) => child.props.originalType != "p"
    );
  }

  return (
    <li
      className={classnames({
        "field-required": required,
        "field-collapsed": collapse,
      })}
      required={required}
    >
      <div className="badges">
        {templateable && (
          <span
            className="badge badge--primary with-info-icon"
            title="This option is dynamic and accepts the Vector template syntax"
          >
            templateable
          </span>
        )}
        {type && (
          <span className="badge badge--secondary">
            {type}
            {unit && <> ({unit})</>}
          </span>
        )}
        {enumValues && Object.keys(enumValues).length > 0 && (
          <span
            className="badge badge--secondary with-info-icon"
            title="This option is an enumation and only allows specific values"
          >
            enum
          </span>
        )}
        {common && (
          <span
            className="badge badge--primary with-info-icon"
            title="This is a popular that we recommend for getting started"
          >
            common
          </span>
        )}
        {required ? (
          <span className="badge badge--danger">
            required{relevantWhen && "*"}
          </span>
        ) : (
          <span className="badge badge--secondary">optional</span>
        )}
      </div>
      {filteredChildren}
      {!collapse && type != "table" && (
        <FieldFooter
          defaultValue={defaultValue}
          enumValues={enumValues}
          examples={examples}
          groups={groups}
          name={name}
          path={path}
          relevantWhen={relevantWhen}
          required={required}
          unit={unit}
          warnings={warnings}
        />
      )}
    </li>
  );
}

export default Field;
