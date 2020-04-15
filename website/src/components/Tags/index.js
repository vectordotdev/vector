import React from 'react';

import Link from '@docusaurus/Link';
import Tag from '@site/src/components/Tag';

import classnames from 'classnames';
import {enrichTags} from '@site/src/exports/tags';
import styles from './styles.module.css';

function Tags({block, colorProfile, tags, valuesOnly}) {
  const enrichedTags = enrichTags(tags, colorProfile);

  return (
    <span className={classnames(styles.tags, {[styles.tagsBlock]: block})}>
      {enrichedTags.map((tag, idx) => (
        <Tag key={idx} valueOnly={valuesOnly} {...tag} />
      ))}
    </span>
  );
}

export default Tags;
