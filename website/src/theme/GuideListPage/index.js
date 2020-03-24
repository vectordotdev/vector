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
  const {metadata, items: guides} = props;
  const context = useDocusaurusContext();
  const {siteConfig = {title: siteTitle}} = context;
  const isGuideOnlyMode = metadata.permalink === '/';
  const title = isGuideOnlyMode ? siteTitle : 'Guides';
  const groupedGuides = _.groupBy(guides, ((guide) => guide.content.metadata.category));

  return (
    <Layout title={title} description="Vector guides, tutorials, and education.">
      <header className="hero hero--clean">
        <div className="container">
          <h1>Vector Guides</h1>
          <div className="hero--subtitle">
            Thoughtful guides to help you get the most out of Vector. Created and curated by the <Link to="/community#team">Vector team</Link>.
          </div>
        </div>
      </header>
      <main className="container">
        {Object.keys(groupedGuides).map((category, index) => {
          let groupGuides = groupedGuides[category];

          return (
            <section>
              <AnchoredH2 id={category}>{humanizeString(category)}</AnchoredH2>
              <GuideItems items={groupGuides.slice(0,25)} staggered={index == 0} />
            </section>
          );
        })}
      </main>
    </Layout>
  );
}

export default GuideListPage;
