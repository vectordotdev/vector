/**
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import React from 'react';

import Layout from '@theme/Layout';
import GuideItems from '@theme/GuideItems';
import Link from '@docusaurus/Link';

function pluralize(count, word) {
  return count > 1 ? `${word}s` : word;
}

function GuideTagsGuidesPage(props) {
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
      <main className="container">
        <GuideItems items={items} />
      </main>
    </Layout>
  );
}

export default GuideTagsGuidesPage;
