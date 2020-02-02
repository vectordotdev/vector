import React from 'react';

import Link from '@docusaurus/Link';

import classnames from 'classnames';
import {enrichTags} from '@site/src/exports/tags';
import styles from './styles.module.css';

function BlogPostTags({tags, valuesOnly}) {
  const enrichedTags = enrichTags(tags);

  return (
    <div className={styles.tags}>
      {enrichedTags.map((tag, idx) => (
        <Link key={idx} to={tag.permalink + '/'} className={classnames('badge', 'badge--rounded', `badge--${tag.style}`)}>{valuesOnly ? tag.value : tag.label}</Link>
      ))}
    </div>
  );
}

export default BlogPostTags;
