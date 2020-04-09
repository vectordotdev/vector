import React from 'react';

import HighlightItem from '@theme/HighlightItem';
import Link from '@docusaurus/Link';

function groupHighlights(items) {
  return _.groupBy(items, ((item) => item.content.frontMatter.release));
}

function Highlight(props) {
  const {content: HighlightContents, index} = props;
  const {frontMatter, metadata} = HighlightContents;
  const {author_github: authorGithub, description, pr_numbers: prNumbers, title} = frontMatter;
  const {date: dateString, permalink, tags} = metadata;

  return (
    <li>
      <HighlightItem
        authorGithub={authorGithub}
        dateString={dateString}
        description={description}
        permalink={permalink}
        prNumbers={prNumbers}
        tags={tags}
        title={title} />
    </li>
  );
}

function HighlightItems({items}) {
  let groupedItems = groupHighlights(items);
  let index = -1;

  return (
    <ul className="connected-list connected-list--timeline">
      {Object.keys(groupedItems).map((release, idx) => {
        let items = groupedItems[release];

        return (
          <>
            <li className="header sticky">
              <Link to={`/releases/${release}/download/`}>{release}</Link>
            </li>
            {items.map((highlight, idx) => {
              index += 1;
              return <Highlight key={idx} index={index} {...highlight} />
            })}
          </>
        );
      })}
    </ul>
  );
}

export default HighlightItems;
