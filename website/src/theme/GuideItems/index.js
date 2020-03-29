import React from 'react';

import GuideItem from '@theme/GuideItem';
import Heading from '@theme/Heading';

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

function GroupedGuideItems({groupLevel, items, large, staggered}) {
  const groupedCategories = _(items).
    map(item => item.content.metadata.categories[groupLevel - 1]).
    uniqBy('permalink').
    sortBy('title').
    keyBy('permalink').
    value();

  const groupedItems = _.groupBy(items, ((item) => item.content.metadata.categories[groupLevel - 1].permalink));
  const SectionHeading = Heading(`h${groupLevel + 1}`);

  return Object.keys(groupedCategories).map((categoryPermalink, index) => {
    let groupItems = groupedItems[categoryPermalink];
    let category = groupedCategories[categoryPermalink];

    return (
      <section key={index}>
        <SectionHeading id={categoryPermalink}>{category.title}</SectionHeading>
        {category.description && <div className="sub-title">{category.description}</div>}
        <GuideItems items={groupItems} large={large} staggered={staggered} />
      </section>
    );
  });
}

function GuideItems({groupLevel, items, large, staggered}) {
  if (groupLevel) {
    return <GroupedGuideItems groupLevel={groupLevel} items={items} />
  } else {
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
}

export default GuideItems;
