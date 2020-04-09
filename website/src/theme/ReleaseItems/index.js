import React from 'react';

import Avatar from '@site/src/components/Avatar';
import Link from '@docusaurus/Link';
import MDXComponents from '@theme/MDXComponents';
import {MDXProvider} from '@mdx-js/react';
import TimeAgo from 'timeago-react';

import _ from 'lodash';
import classnames from 'classnames';
import dateFormat from 'dateformat';

function Release(props) {
  const {content: ReleaseContents} = props;
  const {frontMatter, metadata} = ReleaseContents;
  const {author_github, title} = frontMatter;
  const {date: dateString, description, permalink} = metadata;
  const date = new Date(Date.parse(dateString));

  return (
    <li>
      <Link to={permalink} className={classnames('panel', 'domain-bg', 'domain-bg--hover')}>
        <article>
          <h2>{title}</h2>
          <Avatar
            github={author_github}
            size="sm"
            subTitle={<TimeAgo datetime={date} />}
            rel="author" />
        </article>
      </Link>
    </li>
  );
}

function ReleaseItems({items}) {
  return (
    <ul className="connected-list connected-list--timeline">
      {items.map((release, idx) => (
        <Release key={idx} {...release} />
      ))}
    </ul>
  );
}

export default ReleaseItems;
