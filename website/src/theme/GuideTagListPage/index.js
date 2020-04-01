import React from 'react';

import Heading from '@theme/Heading';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';
import Tag from '@site/src/components/Tag';

import _ from 'lodash';
import {enrichTags} from '@site/src/exports/tags';
import humanizeString from 'humanize-string';
import pluralize from 'pluralize';

const AnchoredH2 = Heading('h2');

function GuideTagListPage(props) {
  const {tags} = props;

  const normalizedTags = Object.values(tags).map(tag => ({
    count: tag.count,
    label: tag.name,
    permalink: tag.permalink
  }));

  const enrichedTags = enrichTags(normalizedTags, 'guides');
  const groupedTags = _.groupBy(enrichedTags, 'category');

  return (
    <Layout title="Tags" description="Vector guide tags">
      <header className="hero hero--clean">
        <div className="container">
          <h1>All Guide Tags</h1>
        </div>
      </header>
      <main className="container container--xs">
        {Object.keys(groupedTags).map((category, index) => {
          let tags = groupedTags[category];
          return (
            <section>
              <AnchoredH2 id={category.name}>{pluralize(humanizeString(category))}</AnchoredH2>

              {tags.map((tag, idx) => (
                <div><Tag key={idx} valueOnly={true} {...tag} /></div>
              ))}
            </section>
          );
        })}
      </main>
    </Layout>
  );
}

export default GuideTagListPage;
