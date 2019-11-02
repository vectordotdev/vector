import React, {useState} from 'react';
import classnames from 'classnames';
import {MDXProvider} from '@mdx-js/react';
import CodeHeader from '@site/src/components/CodeHeader';
import CodeBlock from '@theme/CodeBlock';

import './styles.css';

function isObject(a) {
  return (!!a) && (a.constructor === Object);
};

function toTOML(value) {
  return JSON.stringify(value);
}

function exampleToTOML(name, example) {
  if (isObject(example)) {
    return `${example.name} = ${toTOML(example.value)}`;
  } else if (name) {
    return `${name} = ${toTOML(example)}`;
  } else {
    return `${toTOML(example)}`;
  }
}

function Enum({values}) {
  let elements = [];

  for (var key in values) {
    elements.push(<code key={key} title={values[key]}>{toTOML(key)}</code>);
    elements.push(" ");
  }

  return elements;
}

function Example({name, path, value}) {
  return <code>{exampleToTOML(null, value)}</code>;
}

function Examples({name, path, values}) {
  let code = '';

  values.forEach(function (value) {
    code += (exampleToTOML(name, value) + "\n");
  });

  if (path) {
    code = `[${path}]\n${code}`;
  }

  return (
    <div>
      <CodeHeader fileName="vector.toml" />

      <CodeBlock className="language-toml">
        {code}
      </CodeBlock>
    </div>
  );
}

function RelevantWhen({value}) {
  let relKey = Object.keys(value)[0];
  let relValue = Object.values(value)[0];

  return (
    <span>
      <code>{relKey}</code> = <code>{toTOML(relValue)}</code>
    </span>
  );
}

function OptionFooter({defaultValue, enumValues, examples, name, path, relevantWhen}) {
  const [showExamples, setShowExamples] = useState(false);

  if (defaultValue || enumValues || examples.length > 0) {
    return (
      <div className="info">
        {defaultValue ?
          <div>Default: <Example name={name} path={path} value={defaultValue} /></div> :
          <div>No default</div>}
        {enumValues ?
          <div>Enum, must be one of: <Enum values={enumValues} /></div> :
          null}
        {relevantWhen ?
          <div>Only relevant when: <RelevantWhen value={relevantWhen} /></div> :
          null}
        <div>
          <div className="show-more" onClick={() => setShowExamples(!showExamples)}>
            {showExamples ? "Hide examples" : "View examples"}
          </div>
          {showExamples && <div className="examples"><Examples name={name} path={path} values={examples} /></div>}
        </div>
      </div>
    );
  } else {
    return null;
  }
}

function Option({children, common, defaultValue, depth, enumValues, examples, name, path, relevantWhen, type, unit, required}) {
  return (
    <div className={classnames('option', required ? 'option-required' : '')} required={required}>
      <div className="badges">
        {common && <span className="badge badge--primary" title="Common options are popular options that we recommend for getting started">common</span>}
        <span className="badge badge--secondary">{type}</span>
        {unit && <span className="badge badge--secondary">{unit}</span>}
        {required ?
          <span className="badge badge--danger">required</span> :
          <span className="badge badge--secondary">optional</span>}
      </div>
      {children}
      <OptionFooter defaultValue={defaultValue} enumValues={enumValues} examples={examples} name={name} path={path} relevantWhen={relevantWhen} />
    </div>
  );
}

export default Option;
