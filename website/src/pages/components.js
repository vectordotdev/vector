import React, { useState, useEffect } from 'react';

import VectorComponents from '@site/src/components/VectorComponents';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';

import classnames from 'classnames';
import styles from './components.module.css';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

function Components() {
  return (
    <Layout title="Vector Components">
      <header className={classnames('hero', styles.componentsHero)}>
        <div className="container">
          <h1>Vector Components</h1>
          <div className="hero__subtitle">
            High-quality, reliabile components, allow you to build flexible pipelines. <Link href="/docs/about/concepts">Learn more</Link>.
          </div>
        </div>
      </header>
      <main className="container container--fluid">
        <VectorComponents filterColumn={true} headingLevel={2} />
      </main>
    </Layout>
  );
}

export default Components;