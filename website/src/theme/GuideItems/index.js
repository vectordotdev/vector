import React from 'react';

import GuideItem from '@theme/GuideItem';

import _ from 'lodash';
import classnames from 'classnames';

import './styles.css';

function GuideItems({items, staggered}) {
  let sortedItems = _.sortBy(items, ((guide) => guide.content.metadata.title));

  return (
    <div className="guides">
      <div className={classnames('guide-items', {'guide-items--staggered': staggered})}>
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
