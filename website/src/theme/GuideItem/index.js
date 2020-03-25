import React from 'react';

import Avatar from '@site/src/components/Avatar';
import Link from '@docusaurus/Link';
import MDXComponents from '@theme/MDXComponents';
import {MDXProvider} from '@mdx-js/react';
import Tags from '@site/src/components/Tags';

import classnames from 'classnames';
import dateFormat from 'dateformat';
import {enrichTags} from '@site/src/exports/tags';

import './styles.css';

function GuideItem(props) {
  const {
    children,
    frontMatter,
    metadata,
    truncated,
    isGuidePage = false,
  } = props;
  const {category, description, permalink, readingTime, tags} = metadata;
  const {author_github, last_modified_on: lastModifiedOn, title} = frontMatter;
  const domainTag = enrichTags(tags, 'guides').find(tag => tag.category == 'domain');
  const domain = domainTag ? domainTag.value : null;

  return (
    <Link to={permalink + '/'} className="guide-item domain-bg domain-bg--networking domain-bg--hover">
      <article>
        <header>
          <div className="category">{category}</div>
          <h2 title={title}>{title}</h2>
          <Tags colorProfile="guides" tags={tags} />
        </header>
        <footer>
          <div className="action">read now</div>
        </footer>
      </article>
    </Link>
  );
}

export default GuideItem;
