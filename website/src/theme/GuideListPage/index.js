import React, {useState} from 'react';

import Empty from '@site/src/components/Empty';
import GuideItems from '@theme/GuideItems';
import Heading from '@theme/Heading';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';

import humanizeString from 'humanize-string';
import qs from 'qs';

import './styles.css';

const AnchoredH2 = Heading('h2');

function Guides({filtering, items}) {
  if (items.length == 0) {
    return (
      <Empty text="no guides found" />
    );
  } else if (filtering) {
    return <GuideItems items={items} />
  } else {
    const gettingStartedGuides = items.filter(item => item.content.metadata.categories[0].name == 'getting-started');
    const advancedGuides = items.filter(item => item.content.metadata.categories[0].name == 'advanced');
    const advancedCategory = advancedGuides[0].content.metadata.categories[0];
    const integrationGuides = items.filter(item => item.content.metadata.categories[0].name == 'integrate');
    const integrationCategory = integrationGuides[0].content.metadata.categories[0];

    return (
      <>
        <section>
          <GuideItems items={gettingStartedGuides} staggered={true} />
        </section>
        <section>
          <AnchoredH2 id={advancedCategory.permalink}>{advancedCategory.title}</AnchoredH2>
          {advancedCategory.description && <div className="sub-title">{advancedCategory.description}</div>}
          <GuideItems items={advancedGuides} large={true} />
        </section>
        <section>
          <AnchoredH2 id={integrationCategory.permalink}>{integrationCategory.title}</AnchoredH2>
          {integrationCategory.description && <div className="sub-title">{integrationCategory.description}</div>}
          <GuideItems items={integrationGuides} groupLevel={2} />
        </section>
      </>
    );

    return Object.keys(groupedCategories).map((categoryPermalink, index) => {
      let groupItems = groupedItems[categoryPermalink];
      let category = groupedCategories[categoryPermalink];

      return (
        <section key={index}>
          {index > 0 && <>
            <AnchoredH2 id={categoryPermalink}>{category.title}</AnchoredH2>
            {category.description && <div className="sub-title">{category.description}</div>}
          </>}
          <GuideItems items={groupItems} large={index == 1} staggered={index == 0} />
        </section>
      );
    });
  }
}

function GuideListPage(props) {
  const {metadata, items} = props;
  const queryObj = props.location ? qs.parse(props.location.search, {ignoreQueryPrefix: true}) : {};
  const [searchTerm, setSearchTerm] = useState(queryObj['search']);

  let filtering = false;
  let filteredItems = items.filter(item => {
    let tags = item.content.metadata.tags;
    let hasPlatform = tags.some(tag => tag.label.startsWith('platform: '));
    let hasSource = tags.some(tag => tag.label.startsWith('source: '));
    let hasSink = tags.some(tag => tag.label.startsWith('sink: '));
    return !((hasPlatform || hasSource) && hasSink);
  });

  if (searchTerm) {
    filtering = true;

    filteredItems = filteredItems.filter(item => {
      let normalizedTerm = searchTerm.toLowerCase();
      let frontMatter = item.content.frontMatter;
      let metadata = item.content.metadata;
      let normalizedLabel = metadata.coverLabel.toLowerCase();

      if (normalizedLabel.includes(normalizedTerm)) {
        return true;
      } else if (metadata.tags.some(tag => tag.label.toLowerCase().includes(normalizedTerm))) {
        return true;
      } else {
        return false;
      }
    });
  }

  return (
    <Layout title="Guides" description="Guides, tutorials, and education.">
      <header className="hero hero--clean">
        <div className="container">
          <h1>Vector Guides</h1>
          <div className="hero--subtitle">
            Thoughtful guides to help you get the most out of Vector. Created and curated by the <Link to="/community#team">Vector team</Link>.
          </div>
          <div className="hero--search">
            <input
              type="text"
              className="input--text input--xl"
              onChange={(event) => setSearchTerm(event.currentTarget.value)}
              placeholder="🔍 Search by guide name or tag..." />
          </div>
        </div>
      </header>
      <main className="container container--s">
        <Guides
          filtering={filtering}
          items={filteredItems} />
      </main>
    </Layout>
  );
}

export default GuideListPage;
