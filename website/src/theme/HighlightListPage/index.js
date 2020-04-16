import React, {useState} from 'react';

import HighlightItems from '@theme/HighlightItems';
import Layout from '@theme/Layout';

import qs from 'qs';
import {viewedNewHighlight} from '@site/src/exports/newHighlight';

function HighlightListPage(props) {
  const {items} = props;
  const queryObj = props.location ? qs.parse(props.location.search, {ignoreQueryPrefix: true}) : {};
  const [searchTerm, setSearchTerm] = useState(queryObj['search']);

  viewedNewHighlight();

  //
  // Filter
  //

  let filteredItems = items;

  // Filter breaking changes by default since these will be included in the
  // release notes
  // filteredItems = filteredItems.filter(item => !item.content.metadata.tags.some(tag => tag.label == "type: breaking change"));

  if (searchTerm) {
    filteredItems = filteredItems.filter(item => {
      let normalizedTerm = searchTerm.toLowerCase();
      let frontMatter = item.content.frontMatter;
      let metadata = item.content.metadata;
      let normalizedTitle = metadata.title.toLowerCase();

      if (normalizedTitle.includes(normalizedTerm)) {
        return true;
      } else if (metadata.tags.some(tag => tag.label.toLowerCase().includes(normalizedTerm))) {
        return true;
      } else {
        return false;
      }
    });
  }

  //
  // Render
  //

  return (
    <Layout title="Highlights" description="The latest Vector features and updates.">
      <header className="hero hero--clean">
        <div className="container container--xs">
          <h1>Vector Highlights</h1>
          <div className="hero--subtitle">
            New features &amp; updates. Follow <a href="https://twitter.com/vectordotdev" target="_blank"> <i className="feather icon-twitter"></i> @vectordotdev</a> for real-time updates!
          </div>
          <div className="hero--search">
            <input
              type="text"
              className="input--text input--xl input--block"
              onChange={(event) => setSearchTerm(event.currentTarget.value)}
              placeholder="🔍 Search by title or or tag..." />
          </div>
        </div>
      </header>
      <main className="container container--xs markdown">
        <HighlightItems items={filteredItems} />
      </main>
    </Layout>
  );
}

export default HighlightListPage;
