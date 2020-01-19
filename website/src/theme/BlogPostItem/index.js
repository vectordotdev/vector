import React from 'react';

import Avatar from '@site/src/components/Avatar';
import Link from '@docusaurus/Link';
import MDXComponents from '@theme/MDXComponents';
import {MDXProvider} from '@mdx-js/react';
import Tags from '@site/src/components/Tags';

import classnames from 'classnames';
import dateFormat from 'dateformat';
import {enrichTags} from '@site/src/exports/tags';
import readingTime from 'reading-time';

import './styles.css';

function BlogPostItem(props) {
  const {
    children,
    frontMatter,
    metadata,
    truncated,
    isBlogPostPage = false,
  } = props;
  const {date: dateString, description, permalink, tags} = metadata;
  const {author_github, title} = frontMatter;
  const readingStats = readingTime(children.toString());
  const date = new Date(Date.parse(dateString));
  const domainTag = enrichTags(tags).find(tag => tag.category == 'domain');
  const domain = domainTag ? domainTag.value : null;

  return (
    <Link to={permalink + '/'} className={classnames('blog-post-item', 'domain-bg', 'domain-bg--hover', `domain-bg--${domain}`)}>
      <article>
        <h2>{title}</h2>
        <div className="blog-post-item--subtitle">{description}</div>
        <Avatar github={author_github} size="sm" subTitle={<><time pubdate="pubdate" dateTime={date.toISOString()}>{dateFormat(date, "mmm dS")}</time> / {readingStats.text}</>} rel="author" />
        <Tags tags={tags} />
      </article>
    </Link>
  );
}

export default BlogPostItem;
