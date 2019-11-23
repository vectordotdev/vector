import React from 'react';

import Link from '@docusaurus/Link';

import classnames from 'classnames';

import './styles.css';

function Jump({children, badge, icon, size, target, to}) {
  let classes = classnames('jump-to', `jump-to--${size}`);

  return (
    target ?
      <a href={to} target={target} className={classes}>
        <span className="right">
          {badge ? <span className="badge badge--primary">{badge}</span> : ""}
          <i className={`feather icon-${icon || 'chevron-right'} arrow`}></i>
        </span>
        {children}
      </a> :
      <Link to={to} className={classes}>
        <span className="right">
          {badge ? <span className="badge badge--primary">{badge}</span> : ""}
          <i className={`feather icon-${icon || 'chevron-right'} arrow`}></i>
        </span>
        {children}
      </Link>
  );
}

export default Jump;