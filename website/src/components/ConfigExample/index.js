import React from 'react';

import CodeBlock from '@theme/CodeBlock';
import CodeExplanation from '@site/src/components/CodeExplanation';
import Link from '@docusaurus/Link';
import TabItem from '@theme/TabItem';
import Tabs from '@theme/Tabs';

import _ from 'lodash';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

function Command({format, path, source, sink}) {
  let command = `cat <<-VECTORCFG > ${path}\n${source.config_examples.toml}\n\n${sink.config_examples.toml}\nVECTORCFG`;

  return (
    <>
      <CodeBlock className="language-bash">
        {command}
      </CodeBlock>
      <CodeExplanation>
        <ul>
          <li>The <Link to={`/docs/reference/sources/${source.name}/`}><code>{source.name}</code> source</Link> ingests {source.through_description}.</li>
          <li>The <Link to={`/docs/reference/sinks/${sink.name}/`}><code>{sink.name}</code> sink</Link> writes to {sink.write_to_description}.</li>
          <li>The <code>{path}</code> file is the <Link to="/docs/setup/configuration/">Vector configuration file</Link> that Vector uses in the next step.</li>
        </ul>
      </CodeExplanation>
    </>
  );
}

function ConfigExample({compatiableSinks, format, path, sourceName, sinkName}) {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {sources: sourcesMap, sinks: sinksMap}} = siteConfig.customFields;
  const sources = _.sortBy(Object.values(sourcesMap), ['title'])
  const sinks = _.sortBy(Object.values(sinksMap), ['title']);

  if (sourceName && sinkName) {
    const source = sourcesMap[sourceName];
    const sink = sinksMap[sinkName];
    return <Command format={format} path={path} source={source} sink={sinksMap[sink.name]} />
  } else if (sourceName) {
    const source = sourcesMap[sourceName];
    const compatibleSinks = sinks.filter(sink => source.output_types.some(event_type => sink.input_types.includes(event_type)));

    return (
       <>
        <Tabs
          block={true}
          select={true}
          label="Where do you want to send your data?"
          defaultValue="console"
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
    const sink = sinksMap[sinkName];
    const compatibleSources = sources.filter(source => (
      source.function_category != "test") &&
        sink.input_types.some(event_type => source.output_types.includes(event_type)
    ));

    return (
       <>
        <Tabs
          block={true}
          select={true}
          label="Where do you receive your data from?"
          values={compatibleSources.map(source => ({label: source.title, value: source.name}))}>
          {compatibleSources.map((source, idx) => {
            return (
              <TabItem value={source.name}>
                <Command format={format} path={path} sink={sink} source={sourcesMap[source.name]} />
              </TabItem>
            );
          })}
        </Tabs>
      </>
    );
  } else {
    throw Error('ConfigExample must specify a source or a sink');
  }
}

export default ConfigExample;
