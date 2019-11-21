import React from 'react';
import classnames from 'classnames';

import './styles.css';

function Alert({children, type}) {
  let icon = null;

  switch (type) {
    case 'warning':
      icon = 'alert-triangle';

    default:
      icon = 'info';
  }

  return (
    <div className={classnames('alert', `alert--${type}`)} role="alert">
      <i className={classnames('feather', `icon-${icon}`, `text--${type}`)}></i>
      {children}
    </div>
  );
}

export default Alert;