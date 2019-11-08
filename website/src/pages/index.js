/**
 * Copyright (c) 2017-present, Facebook, Inc.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import React from 'react';
import classnames from 'classnames';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import useBaseUrl from '@docusaurus/useBaseUrl';
import styles from './index.module.css';
import Diagram from '@site/src/components/Diagram';
import repoUrl from '@site/src/exports/repoUrl';

const features = [
  {
    title: 'Blistering Fast',
    imageUrl: 'img/undraw_docusaurus_mountain.svg',
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
    imageUrl: 'img/undraw_docusaurus_tree.svg',
    description: (
      <>
        Vector does not favor any specific storage. It fosters a fair, open ecosystem with the user's best interest in mind.
      </>
    ),
  },
  {
    title: 'Agent or Service',
    imageUrl: 'img/undraw_docusaurus_react.svg',
    description: (
      <>
        Vector aims to be the single, and only, tool needed to get data from A to B, deploying as an <Link to="/docs/setup/deployment/roles/agent">agent</Link> or <Link to="/docs/setup/deployment/roles/service">service</Link>.
      </>
    ),
  },
  {
    title: 'Logs, Metrics, & Events',
    imageUrl: 'img/undraw_docusaurus_react.svg',
    description: (
      <>
        Vector unifies logs, metrics, and events at the source, making it collect and ship all observability data.
      </>
    ),
  },
  {
    title: 'Programmable Transforms',
    imageUrl: 'img/undraw_docusaurus_react.svg',
    description: (
      <>
        An <Link to="/docs/components/transforms/lua">embedded LUA engine</Link> makes it easy to program powerful transforms. Handle complex use cases without limitations.
      </>
    ),
  },
  {
    title: 'Clear Guarantees',
    imageUrl: 'img/undraw_docusaurus_react.svg',
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

function Feature({imageUrl, title, description}) {
  const imgUrl = useBaseUrl(imageUrl);
  return (
    <div className={classnames('col col--4', styles.feature)}>
      {imgUrl && (
        <div className="text--center">
          <img className={styles.featureImage} src={imgUrl} alt={title} />
        </div>
      )}
      <h3>{title}</h3>
      <p>{description}</p>
    </div>
  );
}

function Home() {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  return (
    <Layout
      title={`${siteConfig.title}: ${siteConfig.tagline}`}
      description={siteConfig.description}>
      <div className="fixed-width">
        <header className={classnames('hero', styles.hero)}>
          <div className="container">
            <h1 className={styles.heroH1}>Vector Makes Observability Data Simple</h1>
            <p className="hero__subtitle">
              Vector is an <a href={repoUrl()}>open-source</a> utility for
              collecting, transforming, and routing logs, metrics, and events.
            </p>
            <div className="hero__buttons">
              <button className="button button--primary">Get Started</button>
              <button className="button button--primary">Get Started</button>
            </div>
            <Diagram className={styles.heroDiagram} width="100%" />
          </div>
        </header>
        <main>
          {features && features.length && <Features features={features} />}
        </main>
      </div>
    </Layout>
  );
}

export default Home;
