import React from 'react';

import BlogPostTags from '@site/src/components/BlogPostTags';
import Link from '@docusaurus/Link';

import classnames from 'classnames';
import dateFormat from 'dateformat';
import {enrichTags} from '@site/src/exports/tags';
import readingTime from 'reading-time';

import './styles.css';

function GuideItem(props) {
  const {
    children,
    frontMatter,
    metadata,
  } = props;
  const {date: dateString, description, permalink} = metadata;
  const {title} = frontMatter;
  const readingStats = readingTime(children.toString());
  const date = new Date(Date.parse(dateString));

  return (
    <div className='col col--4'>
      <Link to={permalink + '/'} className={classnames('guide-post-item', 'domain-bg', 'domain-bg--hover')}>
        <article>
          <h2>{title}</h2>
          <div className="guide-post-item--subtitle">{description}</div>
          <><time pubdate="pubdate" dateTime={date.toISOString()}>{dateFormat(date, "mmm dS")}</time> / {readingStats.text}</>
        </article>
      </Link>
    </div>
  );
}

export default GuideItem;
