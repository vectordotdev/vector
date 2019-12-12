import React from 'react';

import Avatar from '@site/src/components/Avatar';
import BlogPostTags from '@site/src/components/BlogPostTags';
import Link from '@docusaurus/Link';
import MDXComponents from '@theme/MDXComponents';
import {MDXProvider} from '@mdx-js/react';

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
  const {author_id, title} = frontMatter;
  const readingStats = readingTime(children.toString());
  const date = Date.parse(dateString);
  const domainTag = enrichTags(tags).find(tag => tag.category == 'domain');
  const domain = domainTag ? domainTag.value : null;

  return (
    <div className={classnames('blog-post-item', 'domain-bg', 'domain-bg--hover', `domain-bg--${domain}`)}>
      <h2><Link to={permalink}>{title}</Link></h2>
      <div className="blog-post-item--subtitle">{description}</div>
      <Avatar id={author_id} size="sm" subTitle={`${dateFormat(date, "mmm dS")} / ${readingStats.text}`} />
      <BlogPostTags tags={tags} />
    </div>
  );
}

export default BlogPostItem;
