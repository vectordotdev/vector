import React from 'react';
import classnames from 'classnames';

import './styles.css';

function Alert({children, fill, icon, type}) {
  let typeIcon = null;

  switch (type) {
    case 'danger':
      typeIcon = 'alert-triangle';
      break;
      
    case 'success':
      typeIcon = 'check-circle';
      break;

    case 'warning':
      typeIcon = 'alert-triangle';
      break;

    default:
      typeIcon = 'info';
  }

  return (
    <div className={classnames('alert', `alert--${type}`, {'alert--fill': fill})} role="alert">
      <i className={classnames('feather', `icon-${icon || typeIcon}`)}></i>
      {children}
    </div>
  );
}

export default Alert;
