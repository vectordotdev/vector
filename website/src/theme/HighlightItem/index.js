import React from 'react';

import Avatar from '@site/src/components/Avatar';
import Link from '@docusaurus/Link';
import Tags from '@site/src/components/Tags';
import TimeAgo from 'timeago-react';

import _ from 'lodash';
import classnames from 'classnames';
import dateFormat from 'dateformat';
import {enrichTags} from '@site/src/exports/tags';

function groupHighlights(items) {
  return _.groupBy(items, ((item) => item.content.frontMatter.release));
}

function prTags(numbers) {
  return numbers.map(number => ({
    enriched: true,
    label: <><i className="feather icon-git-pull-request"></i> {number}</>,
    permalink: `https://github.com/timberio/vector/pull/${number}`,
    style: 'secondary'
  }));
}

function HighlightItem({authorGithub, dateString, description, headingDepth, hideAuthor, hideTags, permalink, prNumbers, style, tags, title}) {
  const date = new Date(Date.parse(dateString));
  const formattedDate = dateFormat(date, "mmm dS, yyyy");
  let enrichedTags = enrichTags(tags, 'highlights');
  enrichedTags = enrichedTags.concat(prTags(prNumbers));
  const domainTag = enrichedTags.find(tag => tag.category == 'domain');
  const domain = domainTag ? domainTag.value : null;
  const HeadingTag = `h${headingDepth || 3}`;

  const subTitle = <>
    <span className="time">
      <span className="formatted-time">{formattedDate}</span>
      <span className="separator"> / </span>
      <TimeAgo title={formattedDate} pubdate="pubdate" datetime={date} />
    </span>
    <span className="separator"> / </span>
    <span className="author-title">Vector core team</span>
  </>;

  return (
    <Link to={permalink} className={classnames('panel', `panel--${style}`, 'domain-bg', 'domain-bg--hover', `domain-bg--${domain}`)}>
      <article>
        <HeadingTag>{title}</HeadingTag>
        <div className="subtitle">{description}</div>
        {!hideAuthor && authorGithub && <Avatar
          github={authorGithub}
          size="sm"
          subTitle={subTitle}
          rel="author" />}
        {!hideTags && enrichedTags.length > 0 && <div>
          <Tags colorProfile="blog" tags={enrichedTags} />
        </div>}
      </article>
    </Link>
  );
}

export default HighlightItem;
