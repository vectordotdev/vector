import React from 'react';

import GuideItems from '@theme/GuideItems';
import Heading from '@theme/Heading';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';

import humanizeString from 'humanize-string';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

import './styles.css';

const AnchoredH2 = Heading('h2');

function GuideListPage(props) {
  const {metadata, items} = props;
  const context = useDocusaurusContext();
  const {siteConfig: {customFields, title: siteTitle}} = context;
  const {metadata: {guides}} = customFields;
  const isGuideOnlyMode = metadata.permalink === '/';
  const title = isGuideOnlyMode ? siteTitle : 'Guides';
  const groupedItems = _.groupBy(items, ((item) => item.content.metadata.category));

  return (
    <Layout title={title} description="Guides, tutorials, and education.">
      <header className="hero hero--clean">
        <div className="container">
          <h1>Vector Guides</h1>
          <div className="hero--subtitle">
            Thoughtful guides to help you get the most out of Vector. Created and curated by the <Link to="/community#team">Vector team</Link>.
          </div>
        </div>
      </header>
      <main className="container">
        {Object.keys(groupedItems).map((categoryName, index) => {
          let category = guides[categoryName];
          let groupItems = groupedItems[categoryName];

          return (
            <section>
              {index > 0 && <>
                <AnchoredH2 id={category.name}>{category.title}</AnchoredH2>
                <div className="sub-title">{category.description}</div>
              </>}
              <GuideItems items={groupItems.slice(0,25)} staggered={index == 0} />
            </section>
          );
        })}
      </main>
    </Layout>
  );
}

export default GuideListPage;
