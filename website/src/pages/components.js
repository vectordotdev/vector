import React, { useState, useEffect } from 'react';

import VectorComponents from '@site/src/components/VectorComponents';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';

import animatedGraph from '@site/src/exports/animatedGraph';
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
    <Layout title="Components - Sources, Transforms, & Sinks" description="Browse and search all of Vector's components: sources, transforms, and sinks. Filter by event type, guarantee, function, operating system, and provider.">
      <header className="hero hero--animated-graph">
        <div className="container container--fluid container--flush">
          <canvas width="2000" height="200"></canvas>
          <div className="overlay">
            <h1>Vector Components</h1>
            <div className="hero--subtitle">
              Components allow you to collect, transform, and route data with ease. <Link to="/docs/about/concepts/">Learn more</Link>.
            </div>
          </div>
        </div>
      </header>
      <main className="container">
        <VectorComponents filterColumn={true} headingLevel={2} location={props.location} />
      </main>
    </Layout>
  );
}

export default Components;
