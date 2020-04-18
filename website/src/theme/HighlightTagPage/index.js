import React from 'react';

import HighlightItems from '@theme/HighlightItems';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';

function pluralize(count, word) {
  return count > 1 ? `${word}s` : word;
}

function HighlightTagPage(props) {
  const {metadata, items} = props;
  const {allTagsPath, name: tagName, count} = metadata;

  return (
    <Layout
      title={`Highlights tagged "${tagName}"`}
      description={`Highlight | Tagged "${tagName}"`}>
      <header className="hero hero--clean">
        <div className="container">
          <h1>{count} {pluralize(count, 'highlight')} tagged with &quot;{tagName}&quot;</h1>
          <div className="hero--subtitle">
            <Link href={allTagsPath}>View All Tags</Link>
          </div>
        </div>
      </header>
      <main className="container container--xs">
        <HighlightItems items={items} />
      </main>
    </Layout>
  );
}

export default HighlightTagPage;
