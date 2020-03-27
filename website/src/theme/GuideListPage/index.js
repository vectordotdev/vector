import React, {useState} from 'react';

import Empty from '@site/src/components/Empty';
import GuideItems from '@theme/GuideItems';
import Heading from '@theme/Heading';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';

import humanizeString from 'humanize-string';
import qs from 'qs';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

import './styles.css';

const AnchoredH2 = Heading('h2');

function Guides({filtering, guidesMetadata, items}) {
  if (items.length == 0) {
    return (
      <Empty text="no guides found" />
    );
  } else if (filtering) {
    return <GuideItems items={items.slice(0,25)} />
  } else {
    const groupedItems = _.groupBy(items, ((item) => item.content.metadata.category));

    return Object.keys(groupedItems).map((categoryName, index) => {
      let category = guidesMetadata[categoryName];
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
    });
  }
}

function GuideListPage(props) {
  const {metadata, items} = props;
  const context = useDocusaurusContext();
  const {siteConfig: {customFields, title: siteTitle}} = context;
  const {metadata: {guides: guidesMetadata}} = customFields;
  const isGuideOnlyMode = metadata.permalink === '/';
  const title = isGuideOnlyMode ? siteTitle : 'Guides';

  const queryObj = props.location ? qs.parse(props.location.search, {ignoreQueryPrefix: true}) : {};
  const [searchTerm, setSearchTerm] = useState(queryObj['search']);

  let filtering = false;
  let filteredItems = items;

  if (searchTerm) {
    filtering = true;

    filteredItems = filteredItems.filter(item => {
      let normalizedTerm = searchTerm.toLowerCase();
      let frontMatter = item.content.frontMatter;
      let metadata = item.content.metadata;
      let normalizedTitle = frontMatter.title.toLowerCase();

      if (normalizedTitle.includes(normalizedTerm)) {
        return true;
      } else if (metadata.tags.some(tag => tag.label.toLowerCase().includes(normalizedTerm))) {
        return true;
      } else {
        return false;
      }
    });
  }

  return (
    <Layout title={title} description="Guides, tutorials, and education.">
      <header className="hero hero--clean">
        <div className="container">
          <h1>Vector Guides</h1>
          <div className="hero--subtitle">
            Thoughtful guides to help you get the most out of Vector. Created and curated by the <Link to="/community#team">Vector team</Link>.
          </div>
          <div className="hero--search">
            <input
              type="text"
              className="input--xl"
              onChange={(event) => setSearchTerm(event.currentTarget.value)}
              placeholder="🔍 Search by guide name or tag..." />
          </div>
        </div>
      </header>
      <main className="container container--s">
        <Guides
          filtering={filtering}
          guidesMetadata={guidesMetadata}
          items={filteredItems} />
      </main>
    </Layout>
  );
}

export default GuideListPage;
