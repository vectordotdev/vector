/**
 * Copyright (c) 2017-present, Facebook, Inc.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import React, { useState, useEffect } from 'react';

import CodeBlock from '@theme/CodeBlock';
import Diagram from '@site/src/components/Diagram';
import Jump from '@site/src/components/Jump';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import PerformanceTests from '@site/src/components/PerformanceTests';
import TabItem from '@theme/TabItem';
import Tabs from '@theme/Tabs';

import classnames from 'classnames';
import {fetchNewPost} from '@site/src/exports/newPost';
import {fetchNewRelease} from '@site/src/exports/newRelease';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import useBaseUrl from '@docusaurus/useBaseUrl';
import repoUrl from '@site/src/exports/repoUrl';
import cloudify from '@site/src/exports/cloudify';

import styles from './index.module.css';
import './index.css';

const features = [
  {
    title: 'Blistering Fast',
    icon: 'zap',
    description: (
      <>
        Built in Rust, Vector is <a href="#performance">blistering fast and
        memory efficient</a>. It's designed to handle the most demanding
        environments.
      </>
    ),
  },
  {
    title: 'Vendor Neutral',
    icon: 'unlock',
    description: (
      <>
        Vector does not favor any specific storage. It fosters a fair, open ecosystem with the user's best interest in mind.
      </>
    ),
  },
  {
    title: 'Agent or Service',
    icon: 'codepen',
    description: (
      <>
        Vector aims to be the single, and only, tool needed to get data from A to B, <Link to="/docs/setup/deployment">deploying</Link> as an <Link to="/docs/setup/deployment/roles/agent">agent</Link> or <Link to="/docs/setup/deployment/roles/service">service</Link>.
      </>
    ),
  },
  {
    title: 'Logs, Metrics, & Events',
    icon: 'shuffle',
    description: (
      <>
        Vector unifies <Link to="/docs/about/data-model/log">logs</Link>, <Link to="/docs/about/data-model/metric">metrics</Link>, and <Link to="/docs/about/data-model#event">events</Link> at the source, making it easy to collect and ship all observability data.
      </>
    ),
  },
  {
    title: 'Programmable Transforms',
    icon: 'code',
    description: (
      <>
        An <Link to="/docs/components/transforms/lua">embedded LUA engine</Link> makes it easy to program powerful transforms. Handle complex use cases without limitations.
      </>
    ),
  },
  {
    title: 'Clear Guarantees',
    icon: 'shield',
    description: (
      <>
        Vector is <Link to="/docs/about/guarantees">clear on it's guarantees</Link>, helping you to make the appropriate trade offs for your use case.
      </>
    ),
  },
];

function Features({features}) {
  let rows = [];

  let i,j,temparray,chunk = 3;
  for (i=0,j=features.length; i<j; i+=chunk) {
    let featuresChunk = features.slice(i,i+chunk);
    
    rows.push(
      <div key={`features${i}`} className="row">
        {featuresChunk.map((props, idx) => (
          <Feature key={idx} {...props} />
        ))}
      </div>
    );
  }

  return (
    <section className={styles.features}>
      <div className="container">
        <h2>Features</h2>
        {rows}
      </div>
    </section>
  );
}

function Feature({icon, title, description}) {
  return (
    <div className={classnames('col col--4', styles.feature)}>
      <div className={styles.featureIcon}>
        <i className={classnames('feather', `icon-${icon}`)}></i>
      </div>
      <h3>{title}</h3>
      <p>{description}</p>
    </div>
  );
}

function Performance() {
  return (
    <section className={styles.performance}>
      <div className="container">
        <h2>Performance</h2>
        <div className="sub-title">Higher throughout with the lowest memory footprint</div>

        <PerformanceTests />
      </div>
    </section>
  );
}

function Correctness() {
  return (
    <section className={styles.correctness}>
      <div className="container">
        <h2>Correctness</h2>
        <div className="sub-title">Obsessed with the details</div>

        <div className="table-responsive">
          <table className="comparison">
            <thead>
              <tr>
                <th></th>
                <th>Vector</th>
                <th>Filebeat</th>
                <th>FluentBit</th>
                <th>FluentD</th>
                <th>Logstash</th>
                <th>SplunkHF</th>
                <th>SplunkUF</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td><a target="_blank" href="https://github.com/timberio/vector-test-harness/tree/master/cases/disk_buffer_persistence_correctness">Disk buffer persistence</a></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="failed"><i className="feather icon-x"></i></td>
                <td className="failed"><i className="feather icon-x"></i></td>
                <td className="warning"><i className="feather icon-alert-triangle"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
              </tr>
              <tr>
                <td><a target="_blank" href="https://github.com/timberio/vector-test-harness/tree/master/cases/file_rotate_create_correctness">File rotate (create)</a></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
              </tr>
              <tr>
                <td><a target="_blank" href="https://github.com/timberio/vector-test-harness/tree/master/cases/file_rotate_truncate_correctness">File rotate (copytruncate)</a></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="failed"><i className="feather icon-x"></i></td>
                <td className="failed"><i className="feather icon-x"></i></td>
                <td className="failed"><i className="feather icon-x"></i></td>
                <td className="failed"><i className="feather icon-x"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
              </tr>
              <tr>
                <td><a target="_blank" href="https://github.com/timberio/vector-test-harness/tree/master/cases/file_truncate_correctness">File truncation</a></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
              </tr>
              <tr>
                <td><a target="_blank" href="https://github.com/timberio/vector-test-harness/tree/master/cases/sighup_correctness">Process (SIGHUP)</a></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="failed"><i className="feather icon-x"></i></td>
                <td className="failed"><i className="feather icon-x"></i></td>
                <td className="failed"><i className="feather icon-x"></i></td>
                <td className="warning"><i className="feather icon-alert-triangle"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
              </tr>
              <tr>
                <td><a target="_blank" href="https://github.com/timberio/vector-test-harness/tree/master/cases/tcp_stream_correctness">TCP Streaming</a></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="failed"><i className="feather icon-x"></i></td>
                <td className="failed"><i className="feather icon-x"></i></td>
                <td className="failed"><i className="feather icon-x"></i></td>
                <td className="failed"><i className="feather icon-x"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
              </tr>
              <tr>
                <td><a target="_blank" href="https://github.com/timberio/vector-test-harness/tree/master/cases/wrapped_json_correctness">JSON (wrapped)</a></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="failed"><i className="feather icon-x"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
                <td className="passed"><i className="feather icon-check"></i></td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </section>
  );
}

function Configuration() {
  return (
    <section className={styles.correctness}>
      <div className="container">
        <h2>Configuration</h2>

        
      </div>
    </section>
  );
}

function Integrations() {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {sources, transforms, sinks}} = siteConfig.customFields;

  const classes = {
    'aws_s3_sink': 'large',
    'clickhouse_sink': 'medium',
    'docker_source': 'large',
    'elasticsearch_sink': 'large',
    'file_source': 'medium',
    'http_sink': 'small',
    'kafka_source': 'large',
    'log_to_metric_transform': 'large',
    'lua_transform': 'medium',
    'prometheus_sink': 'large',
    'regex_parser': 'medium',
    'syslog_source': 'medium',
  }

  return (
    <section className={classnames(styles.integrations, 'integrations')}>
      <div className="container">
        <h2>Integrates With Everything</h2>
        <div className="sub-title">Sources, transforms, and sinks make it easy to compose pipelines</div>

        <div className={classnames(styles.components, 'components')}>
          <h3>
            <div>
              <span className="line-break">{Object.keys(sources).length} sources</span>
              <span className="line-break">{Object.keys(transforms).length} transforms</span>
              <span className="line-break">{Object.keys(sinks).length} sinks</span>
            </div>
          </h3>
          <div className={styles.componentsCanvas} id="component-canvas"></div>
          <ul>
            {Object.keys(sources).map((key, index) => (
              <li className={classes[`${key}_source`]} key={index}><Link to={`/docs/components/sources/${key}`}>{sources[key].name}</Link></li>
            ))}
            {Object.keys(transforms).map((key, index) => (
              <li className={classes[`${key}_transform`]} key={index}><Link to={`/docs/components/transforms/${key}`}>{transforms[key].name}</Link></li>
            ))}
            {Object.keys(sinks).map((key, index) => (
              <li className={classes[`${key}_sink`]} key={index}><Link to={`/docs/components/sinks/${key}`}>{sinks[key].name}</Link></li>
            ))}
          </ul>
        </div>
      </div>
    </section>
  )
}

function Installation() {
  return (
    <section className={styles.installation}>
      <div className="container">
        <h2>Installs Everywhere</h2>
        <div className="sub-title">Fully static, no dependencies, no runtime, memory safe</div>

        <Tabs
          block={true}
          defaultValue="humans"
          values={[
            { label: <><i className="feather icon-user-check"></i> For Humans</>, value: 'humans', },
            { label: <><i className="feather icon-cpu"></i> For Machines</>, value: 'machines', },
          ]
        }>
          <TabItem value="humans">
            <CodeBlock className="language-bash">
              curl --proto '=https' --tlsv1.2 -sSf https://sh.vector.dev | sh
            </CodeBlock>
          </TabItem>
          <TabItem value="machines">
            <CodeBlock className="language-bash">
              curl --proto '=https' --tlsv1.2 -sSf https://sh.vector.dev | sh -s -- -y
            </CodeBlock>
          </TabItem>
        </Tabs>

        <h3 className={styles.installSubTitle}>Or choose your preferred method:</h3>

        <div className="row">
          <div className="col">
            <Jump to="/docs/setup/installation/containers">Containers</Jump>
          </div>
          <div className="col">
            <Jump to="/docs/setup/installation/package-managers">Package Managers</Jump>
          </div>
          <div className="col">
            <Jump to="/docs/setup/installation/operating-systems">Operating Systems</Jump>
          </div>
          <div className="col">
            <Jump to="/docs/setup/installation/manual">Manual</Jump>
          </div>
        </div>
      </div>
    </section>
  );
}

function Home() {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {latest_release}} = siteConfig.customFields;
  const newPost = fetchNewPost();
  const newRelease = fetchNewRelease();

  useEffect(() => {
    cloudify();
  }, []);

  return (
    <Layout
      title={`${siteConfig.title}: ${siteConfig.tagline}`}
      description={siteConfig.description}>
      <header className={classnames('hero', styles.indexHeroBanner)}>
        <div className="container">
          {newRelease && (
            <a href="/" className={styles.indexAnnouncement}>
              <span className="badge badge-primary">new</span>
              v{newRelease.version} has been released! Download now.
            </a>
          )}
          {!newRelease && newPost && (
            <a href="/" className={styles.indexAnnouncement}>
              <span className="badge badge-primary">new</span>
              {newPost.title}
            </a>
          )}
          <h1>Vector Makes Observability Data Simple</h1>
          <p className="hero__subtitle">
            Vector is an <a href={repoUrl()}>open-source</a> utility for
            collecting, transforming, and routing logs, metrics, and events.
          </p>
          <div className="hero__buttons">
            <Link to="/docs/setup/guides/getting-started" className="button button--primary">Get Started</Link>
            <Link to="/download" className="button button--primary">Download v{latest_release.version}</Link>
          </div>
          <Diagram className={styles.indexHeroDiagram} width="100%" />
        </div>
      </header>
      <main>
        {features && features.length && <Features features={features} />}
        <Performance />
        <Correctness />
        <Integrations />
        <Installation />
      </main>
    </Layout>
  );
}

export default Home;
