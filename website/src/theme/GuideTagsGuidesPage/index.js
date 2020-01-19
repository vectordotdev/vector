/**
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import React from 'react';

import Layout from '@theme/Layout';
import GuideItem from '@theme/GuideItem';
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
      <div className="container margin-vert--xl">
        <div className="row">
          <div className="col col--8 col--offset-2">
            <h1>
              {count} {pluralize(count, 'guide')} tagged with &quot;{tagName}
              &quot;
            </h1>
            <Link href={allTagsPath}>View All Tags</Link>
            <div className="margin-vert--xl">
              {items.map(({content: GuideContent}) => (
                <GuideItem
                  key={GuideContent.metadata.permalink}
                  frontMatter={GuideContent.frontMatter}
                  metadata={GuideContent.metadata}
                  truncated>
                  <GuideContent />
                </GuideItem>
              ))}
            </div>
          </div>
        </div>
      </div>
    </Layout>
  );
}

export default GuideTagsGuidesPage;
