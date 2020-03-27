import React from 'react';

import Layout from '@theme/Layout';
import GuideItems from '@theme/GuideItems';
import Link from '@docusaurus/Link';

import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

function GuideTagPage(props) {
  const {metadata: {category: categoryName}, items} = props;
  const context = useDocusaurusContext();
  const {siteConfig: {customFields, title: siteTitle}} = context;
  const {metadata: {guides: guidesMetadata}} = customFields;
  const category = guidesMetadata[categoryName];

  return (
    <Layout
      title={`${category.title} Guides`}
      description={category.description}>
      <header className="hero hero--clean">
        <div className="container">
          <h1>{category.title} Guides</h1>
          <div className="hero--subtitle">{category.description}</div>
          <div><Link to="/guides">View All Guides</Link></div>
        </div>
      </header>
      <main className="container">
        <GuideItems items={items} staggered={category.series} />
      </main>
    </Layout>
  );
}

export default GuideTagPage;
