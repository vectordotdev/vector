import React, { useState, useEffect } from 'react';

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
          <h1>Components</h1>
          <div className="hero__subtitle">fdsf</div>
        </div>
      </header>
      <main className={classnames('container')}>
        <div className="row">
          <div className={classnames('col', 'col--2')}>Sidebar</div>
          <main className={classnames('col')}>
            fdsfsd
          </main>
        </div>
      </main>
    </Layout>
  );
}

export default Components;