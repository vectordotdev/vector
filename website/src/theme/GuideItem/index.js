import React from 'react';

import Avatar from '@site/src/components/Avatar';
import Link from '@docusaurus/Link';
import MDXComponents from '@theme/MDXComponents';
import {MDXProvider} from '@mdx-js/react';
import Tags from '@site/src/components/Tags';

import classnames from 'classnames';
import dateFormat from 'dateformat';

import './styles.css';

function GuideItem(props) {
  const {
    children,
    frontMatter,
    metadata,
    truncated,
    isGuidePage = false,
  } = props;
  const {category, description, domain, permalink, readingTime, seriesPosition, tags} = metadata;
  const {author_github, last_modified_on: lastModifiedOn, title} = frontMatter;

  return (
    <Link to={permalink + '/'} className={`guide-item domain-bg domain-bg--${domain} domain-bg--hover`}>
      <article>
        <header>
          <div className="category">{category}</div>
          <h2 title={title}>{seriesPosition && (seriesPosition + '. ')}{title}</h2>
        </header>
        <footer>
          <Tags colorProfile="guides" tags={tags} />
          <div className="action">read now</div>
        </footer>
      </article>
    </Link>
  );
}

export default GuideItem;
