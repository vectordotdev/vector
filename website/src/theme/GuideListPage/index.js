import React from 'react';

import GuideItems from '@theme/GuideItems';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';

import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

import './styles.css';

function GuideListPage(props) {
  const {metadata, items} = props;
  const context = useDocusaurusContext();
  const {siteConfig = {title: siteTitle}} = context;
  const isGuideOnlyMode = metadata.permalink === '/';
  const title = isGuideOnlyMode ? siteTitle : 'Guides';

  return (
    <Layout title={title} description="Vector guides, tutorials, and education.">
      <header className="hero hero--clean">
        <div className="container">
          <h1>Vector Guides</h1>
          <div className="hero--subtitle">
            Thoughtful guides to help you get the most out of Vector. Created and curated by the Vector team.
          </div>
        </div>
      </header>
      <main className="container">
        <GuideItems items={items.slice(0,25)} staggered={true} />
      </main>
    </Layout>
  );
}

export default GuideListPage;
