import React from 'react';

import Link from '@docusaurus/Link';

import classnames from 'classnames';

function Tag({count, label, permalink, style, value, valueOnly}) {
  return <Link
    to={permalink + '/'}
    className={classnames('badge', 'badge--rounded', `badge--${style}`)}>
    {valueOnly ? value : label}
    {count && <> ({count})</>}
  </Link>;
}

export default Tag;
