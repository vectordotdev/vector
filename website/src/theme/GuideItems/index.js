import React from 'react';

import GuideItem from '@theme/GuideItem';

import _ from 'lodash';
import classnames from 'classnames';

import './styles.css';

function sortGuides(guides) {
  return _.sortBy(
    guides,
    [
      'content.metadata.seriesPosition',
      ((guide) => guide.content.metadata.coverLabel.toLowerCase())
    ]
  )
}

function GuideItems({items, large, staggered}) {
  let sortedItems = sortGuides(items);

  return (
    <div className="guides">
      <div className={classnames('guide-items', {'guide-items--l': large, 'guide-items--staggered': staggered})}>
        {sortedItems.map(({content: GuideContent}) => (
          <GuideItem
            key={GuideContent.metadata.permalink}
            frontMatter={GuideContent.frontMatter}
            metadata={GuideContent.metadata}
            truncated={GuideContent.metadata.truncated}>
            <GuideContent />
          </GuideItem>
        ))}
      </div>
    </div>
  );
}

export default GuideItems;
