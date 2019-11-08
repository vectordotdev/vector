import React from 'react';

import './styles.css';
import Link from '@docusaurus/Link';

function Jump({children, icon, to}) {

  return (
    <Link to={to} className="jump-to">
      <i className={`feather icon-${icon || 'chevron-right'} arrow`}></i>
      {children}</Link>
  );
}

export default Jump;