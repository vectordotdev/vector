import React from 'react';

import CodeBlock from '@theme/CodeBlock';
import CodeExplanation from '@site/src/components/CodeExplanation';
import CodeHeader from '@site/src/components/CodeHeader';
import Link from '@docusaurus/Link';
import TabItem from '@theme/TabItem';
import Tabs from '@theme/Tabs';

import _ from 'lodash';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

function Command({format, path, source, sink}) {
  let command = `echo '\n${source.config_examples.toml}\n\n${sink.config_examples.toml}\n' > ${path}`;

  return (
    <>
      <CodeHeader icon="info" text="adjust the values as necessary" />
      <CodeBlock className="language-bash">
        {command}
      </CodeBlock>
      <CodeExplanation>
        <ul>
          <li>The <Link to={`/docs/reference/sources/${source.name}`}><code>{source.name}</code> source</Link> ingests data.</li>
          <li>The <Link to={`/docs/reference/sinks/${sink.name}`}><code>{sink.name}</code> sink</Link> outputs data.</li>
          <li>The {path} file is the <Link to="/docs/configuration">Vector configuration file</Link> that we'll pass in the next step.</li>
        </ul>
      </CodeExplanation>
    </>
  );
}

function ConfigExample({compatiableSinks, format, path, sourceName, sinkName}) {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {sources: sourcesMap, sinks: sinksMap}} = siteConfig.customFields;
  const sources = Object.values(sourcesMap);
  const sinks = _.sortBy(Object.values(sinksMap), ['title']);

  if (sourceName && sinkName) {
    const source = sourcesMap[sourceName];
    const sink = sinksMap[sinkName];
    return <Command format={format} path={path} source={source} sink={sinksMap[sink.name]} />
  } else if (sourceName) {
    const source = sourcesMap[sourceName];
    const compatibleSinks = sinks.filter(sink => (
      sink.function_category != "test") &&
        source.output_types.some(event_type => sink.input_types.includes(event_type)
    ));

    return (
       <>
        <Tabs
          block={true}
          select={true}
          label="Where would you like to send your data?"
          values={compatibleSinks.map(sink => ({label: sink.title, value: sink.name}))}>
          {compatibleSinks.map((sink, idx) => {
            return (
              <TabItem value={sink.name}>
                <Command format={format} path={path} source={source} sink={sinksMap[sink.name]} />
              </TabItem>
            );
          })}
        </Tabs>
      </>
    );
  } else if (sinkName) {

  } else {

  }
}

export default ConfigExample;
