import React from 'react';

import './styles.css';
import Link from '@docusaurus/Link';

function Jump({children, icon, target, to}) {
  return (
    target ?
      <a href={to} target={target} className="jump-to">
        <i className={`feather icon-${icon || 'chevron-right'} arrow`}></i>
        {children}
      </a> :
      <Link to={to} className="jump-to">
        <i className={`feather icon-${icon || 'chevron-right'} arrow`}></i>
        {children}
      </Link>
  );
}

export default Jump;