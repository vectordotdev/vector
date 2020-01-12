/**
 * Copyright (c) 2017-present, Facebook, Inc.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import React, { useState, useEffect } from 'react';

import CodeBlock from '@theme/CodeBlock';
import Diagram from '@site/src/components/Diagram';
import Heading from '@theme/Heading';
import Jump from '@site/src/components/Jump';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import PerformanceTests from '@site/src/components/PerformanceTests';
import SVG from 'react-inlinesvg';
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

const AnchoredH2 = Heading('h2');

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
        Vector does not favor any storage and fosters a fair, open ecosystem with your best interest in mind. Lock-in free and future proof.
      </>
    ),
  },
  {
    title: 'One Tool',
    icon: 'codepen',
    description: (
      <>
        Vector aims to be the single, and only, tool needed to get data from A to B, <Link to="/docs/setup/deployment">deploying</Link> as an <Link to="/docs/setup/deployment/roles/agent">agent</Link> or <Link to="/docs/setup/deployment/roles/service">service</Link>.
      </>
    ),
  },
  {
    title: 'All Data',
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
        <Link to="/components?functions[]=program">Programmable transforms</Link> give you the full power of programmable runtimes. Handle complex use cases without limitation.
      </>
    ),
  },
  {
    title: 'Clear Guarantees',
    icon: 'shield',
    description: (
      <>
        Guarantees matter, and Vector is <Link to="/docs/about/guarantees">clear on it's guarantees</Link>, helping you to make the appropriate trade offs for your use case.
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
        <AnchoredH2 id="features">Why Vector?</AnchoredH2>
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
        <AnchoredH2 id="performance">Performance That Doesn't Deter</AnchoredH2>
        <div className="sub-title">Built for the most demanding production environments</div>

        <PerformanceTests />
      </div>
    </section>
  );
}

function Correctness() {
  return (
    <section className={styles.correctness}>
      <div className="container">
        <AnchoredH2 id="correctness">Correct To The Smallest Details</AnchoredH2>
        <div className="sub-title">We're obsessed with getting the details right</div>

        <div className="table-responsive">
          <table className="comparison">
            <thead>
              <tr>
                <th></th>
                <th className="vector">Vector</th>
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
                <td className="row-label"><a target="_blank" href="https://github.com/timberio/vector-test-harness/tree/master/cases/disk_buffer_persistence_correctness">Disk buffer persistence</a></td>
                <td className="result passed vector"><i className="feather icon-check"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
                <td className="result failed"><i className="feather icon-x"></i></td>
                <td className="result failed"><i className="feather icon-x"></i></td>
                <td className="result warning"><i className="feather icon-alert-triangle"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
              </tr>
              <tr>
                <td className="row-label"><a target="_blank" href="https://github.com/timberio/vector-test-harness/tree/master/cases/file_rotate_create_correctness">File rotate (create)</a></td>
                <td className="result passed vector"><i className="feather icon-check"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
              </tr>
              <tr>
                <td className="row-label"><a target="_blank" href="https://github.com/timberio/vector-test-harness/tree/master/cases/file_rotate_truncate_correctness">File rotate (copytruncate)</a></td>
                <td className="result passed vector"><i className="feather icon-check"></i></td>
                <td className="result failed"><i className="feather icon-x"></i></td>
                <td className="result failed"><i className="feather icon-x"></i></td>
                <td className="result failed"><i className="feather icon-x"></i></td>
                <td className="result failed"><i className="feather icon-x"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
              </tr>
              <tr>
                <td className="row-label"><a target="_blank" href="https://github.com/timberio/vector-test-harness/tree/master/cases/file_truncate_correctness">File truncation</a></td>
                <td className="result passed vector"><i className="feather icon-check"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
              </tr>
              <tr>
                <td className="row-label"><a target="_blank" href="https://github.com/timberio/vector-test-harness/tree/master/cases/sighup_correctness">Process (SIGHUP)</a></td>
                <td className="result passed vector"><i className="feather icon-check"></i></td>
                <td className="result failed"><i className="feather icon-x"></i></td>
                <td className="result failed"><i className="feather icon-x"></i></td>
                <td className="result failed"><i className="feather icon-x"></i></td>
                <td className="result warning"><i className="feather icon-alert-triangle"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
              </tr>
              <tr>
                <td className="row-label"><a target="_blank" href="https://github.com/timberio/vector-test-harness/tree/master/cases/tcp_stream_correctness">TCP Streaming</a></td>
                <td className="result passed vector"><i className="feather icon-check"></i></td>
                <td className="result failed"><i className="feather icon-x"></i></td>
                <td className="result failed"><i className="feather icon-x"></i></td>
                <td className="result failed"><i className="feather icon-x"></i></td>
                <td className="result failed"><i className="feather icon-x"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
              </tr>
              <tr>
                <td className="row-label"><a target="_blank" href="https://github.com/timberio/vector-test-harness/tree/master/cases/wrapped_json_correctness">JSON (wrapped)</a></td>
                <td className="result passed vector"><i className="feather icon-check"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
                <td className="result failed"><i className="feather icon-x"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
                <td className="result passed"><i className="feather icon-check"></i></td>
              </tr>
            </tbody>
          </table>
        </div>
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
    'socket_sink': 'medium',
    'syslog_source': 'medium',
  }

  return (
    <section className={classnames(styles.integrations, 'integrations')}>
      <div className="container">
        <AnchoredH2 id="integrations">Quality Integrations Built Into The Core</AnchoredH2>
        <div className="sub-title">Actively maintained integrations. Gone are the days of dormant low-quality plugins.</div>

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
              <li className={classes[`${key}_source`]} key={index}><Link to={`/docs/reference/sources/${key}/`}>{sources[key].name}</Link></li>
            ))}
            {Object.keys(transforms).map((key, index) => (
              <li className={classes[`${key}_transform`]} key={index}><Link to={`/docs/reference/transforms/${key}/`}>{transforms[key].name}</Link></li>
            ))}
            {Object.keys(sinks).map((key, index) => (
              <li className={classes[`${key}_sink`]} key={index}><Link to={`/docs/reference/sinks/${key}/`}>{sinks[key].name}</Link></li>
            ))}
          </ul>
        </div>
      </div>
    </section>
  )
}

function Configuration() {
  return (
    <section className="configuration">
      <div className="container">
        <AnchoredH2 id="configuration">Simple To Configure</AnchoredH2>
        <div className="sub-title">A simple composable format lets you build flexible pipelines</div>

        <div className="configuration__diagram">
          <SVG src="/img/configuration.svg" />
        </div>
      </div>
    </section>
  );
}

function Topologies() {
  return (
    <section className="topologies">
      <div className="container">
        <AnchoredH2 id="topologies">One Tool For Your Entire Pipeline</AnchoredH2>
        <div className="sub-title">Get data from A to B without patching tools together</div>

        <Tabs
          centered={true}
          className="rounded"
          defaultValue="centralized"
          values={[
            { label: <><i className="feather icon-shuffle"></i> Distributed</>, value: 'distributed', },
            { label: <><i className="feather icon-box"></i> Centralized</>, value: 'centralized', },
            { label: <><i className="feather icon-shield"></i> Stream-based</>, value: 'stream-based', },
          ]}>
          <TabItem value="distributed">
            <div className={styles.topology}>
              <SVG src="/img/topologies-distributed.svg" className={styles.topologyDiagram} />
              <Link to="/docs/setup/deployment/topologies#distributed">Learn more about the distributed topology</Link>
            </div>
          </TabItem>
          <TabItem value="centralized">
            <div className={styles.topology}>
              <SVG src="/img/topologies-centralized.svg" className={styles.topologyDiagram} />
              <Link to="/docs/setup/deployment/topologies#centralized">Learn more about the centralized topology</Link>
            </div>
          </TabItem>
          <TabItem value="stream-based">
            <div className={styles.topology}>
              <SVG src="/img/topologies-stream-based.svg" className={styles.topologyDiagram} />
              <Link to="/docs/setup/deployment/topologies#stream-based">Learn more about the stream-based topology</Link>
            </div>
          </TabItem>
        </Tabs>
      </div>
    </section>
  )
}

function Installation() {
  return (
    <section className={styles.installation}>
      <div className="container">
        <AnchoredH2 id="installation">Installs Everywhere</AnchoredH2>
        <div className="sub-title">Fully static, no dependencies, no runtime, memory safe</div>

        <div className={styles.installationPlatforms}>
          <Link to="/docs/setup/installation/containers/docker"><SVG src="/img/docker.svg" /></Link>
          <Link to="/docs/setup/installation/operating-systems"><SVG src="/img/linux.svg" /></Link>
          <Link to="/docs/setup/installation/operating-systems/raspbian"><SVG src="/img/raspbian.svg" /></Link>
          <Link to="/docs/setup/installation/operating-systems/windows"><SVG src="/img/windows.svg" /></Link>
          <Link to="/docs/setup/installation/operating-systems/macos"><SVG src="/img/apple.svg" /></Link>
        </div>

        <div className={styles.installationChecks}>
          <div>
            <i className="feather icon-package"></i> Fully static, no deps
          </div>
          <div>
            <i className="feather icon-cpu"></i> X86_64, ARM64, & ARMv7
          </div>
          <div>
            <i className="feather icon-feather"></i> Light-weight, only 7mb
          </div>
          <div>
            <i className="feather icon-zap"></i> No runtime, mem-safe
          </div>
        </div>

        <h3 className={styles.installSubTitle}>Install with a one-liner:</h3>

        <Tabs
          className="mini"
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
            <Jump to="/docs/setup/installation/containers/">Containers</Jump>
          </div>
          <div className="col">
            <Jump to="/docs/setup/installation/package-managers/">Package Managers</Jump>
          </div>
          <div className="col">
            <Jump to="/docs/setup/installation/operating-systems/">Operating Systems</Jump>
          </div>
          <div className="col">
            <Jump to="/docs/setup/installation/manual/">Manual</Jump>
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
    <Layout description={siteConfig.description}>
      <header className={classnames('hero', 'hero--full-height', styles.indexHeroBanner)}>
        <div className="container">
          {newRelease && (
            <Link to={`/releases/${newRelease.version}`} className={styles.indexAnnouncement}>
              <span className="badge badge-primary">new</span>
              v{newRelease.version} has been released! View release notes.
            </Link>
          )}
          {!newRelease && newPost && (
            <Link to={`/blog/${newPost.id}`} className={styles.indexAnnouncement}>
              <span className="badge badge-primary">new</span>
              {newPost.title}
            </Link>
          )}
          <h1>Take Control Of Your Observability Data</h1>
          <p className="hero__subtitle">
            Vector is an <a href={repoUrl()}>open-source</a> utility for building <a href="#topologies">end-to-end</a>, <a href="#performance">high performance</a> observability pipelines.
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
        <Configuration />
        <Integrations />
        <Topologies />
        <Installation />
      </main>
    </Layout>
  );
}

export default Home;
