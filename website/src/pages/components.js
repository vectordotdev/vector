import React, { useState, useEffect } from 'react';

import VectorComponents from '@site/src/components/VectorComponents';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';

import animatedGraph from '@site/src/exports/animatedGraph';
import classnames from 'classnames';
import styles from './components.module.css';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

function Components(props) {
  useEffect(() => {
    if (typeof document !== 'undefined') {
      let canvas = document.querySelector("canvas");
      let timer = animatedGraph(canvas);
      return () => {
        timer.stop();
      }
    }
  }, []);

  return (
    <Layout title="Vector Components" description="Browse and search all Vector components.">
      <header className={classnames('hero', styles.componentsHero)}>
        <div className="container container--fluid">
          <canvas width="2000" height="300"></canvas>
          <div className={styles.componentsHeroOverlay}>
            <h1>Vector Components</h1>
            <div className="hero__subtitle">
              Components allow you to collect, transform, and route data with ease. <Link to="/docs/about/concepts/">Learn more</Link>.
            </div>
          </div>
        </div>
      </header>
      <main className="container container--fluid">
        <VectorComponents filterColumn={true} headingLevel={2} location={props.location} />
      </main>
    </Layout>
  );
}

export default Components;