import React from 'react';

import Link from '@docusaurus/Link';

import classnames from 'classnames';

import './styles.css';

function Jump({children, className, badge, icon, size, target, to}) {
  let classes = classnames('jump-to', `jump-to--${size}`, className);

  let content = (
    <div className="jump-to--inner">
      <div className="jump-to--inner-2">
        <div className="jump-to--main">
          {badge ? <span className="badge badge--primary badge--right">{badge}</span> : ""}
          {children}
        </div>
        <div className="jump-to--right">
          <i className={`feather icon-${icon || 'chevron-right'} arrow`}></i>
        </div>
      </div>
    </div>
  );

  return (
    target ?
      <a href={to} target={target} className={classes}>{content}</a> :
      <Link to={to} className={classes}>{content}</Link>
  );
}

export default Jump;
