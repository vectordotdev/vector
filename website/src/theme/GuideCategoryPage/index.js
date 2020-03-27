import React from 'react';

import Layout from '@theme/Layout';
import GuideItems from '@theme/GuideItems';
import Link from '@docusaurus/Link';

import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

function GuideCategoryPage(props) {
  const {metadata: {categorySlug}, items} = props;
  const context = useDocusaurusContext();
  const {siteConfig: {customFields, title: siteTitle}} = context;
  const {metadata: {guides: guidesMetadata}} = customFields;
  const mainCategory = guidesMetadata[categorySlug.split('/')[0]];

  return (
    <Layout
      title={`${categorySlug} Guides`}
      description={mainCategory.description}>
      <header className="hero hero--clean">
        <div className="container">
          <h1>{categorySlug} Guides</h1>
          <div className="hero--subtitle">{mainCategory.description}</div>
          <div><Link to="/guides">View All Guides</Link></div>
        </div>
      </header>
      <main className="container container--s">
        <GuideItems items={items} staggered={mainCategory.series} />
      </main>
    </Layout>
  );
}

export default GuideCategoryPage;
