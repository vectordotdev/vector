import React from 'react';

import Layout from '@theme/Layout';
import GuideItems from '@theme/GuideItems';
import Link from '@docusaurus/Link';

function pluralize(count, word) {
  return count > 1 ? `${word}s` : word;
}

function GuideTagPage(props) {
  const {metadata, items} = props;
  const {allTagsPath, name: tagName, count} = metadata;

  return (
    <Layout
      title={`Guides tagged "${tagName}"`}
      description={`Guide | Tagged "${tagName}"`}>
      <header className="hero hero--clean">
        <div className="container">
          <h1>{count} {pluralize(count, 'guide')} tagged with &quot;{tagName}&quot;</h1>
          <div className="hero--subtitle">
            <Link href={allTagsPath}>View All Tags</Link>
          </div>
        </div>
      </header>
      <main className="container container--s">
        <GuideItems items={items} />
      </main>
    </Layout>
  );
}

export default GuideTagPage;
