import React from 'react';

import Layout from '@theme/Layout';
import GuideItems from '@theme/GuideItems';
import Link from '@docusaurus/Link';

function GuideCategoryPage(props) {
  const {metadata: {category}, items} = props;

  return (
    <Layout
      title={`${category.title} Guides`}
      description={`All ${category.title} guides`}>
      <header className="hero hero--clean">
        <div className="container">
          <h1>{category.title} Guides</h1>
          {category.description && <div className="hero--subtitle">{category.description}</div>}
          <div><Link to="/guides">View All Guides</Link></div>
        </div>
      </header>
      <main className="container container--s">
        <GuideItems items={items} staggered={items[0].content.metadata.seriesPosition != null} />
      </main>
    </Layout>
  );
}

export default GuideCategoryPage;
