import React from 'react';

import Link from '@docusaurus/Link';

import classnames from 'classnames';

import './styles.css';

function Vic({className, size, style, text}) {
  return <Link to="/vic/" className={classnames('vic', `vic--${size}`, className)}>
    <div className="icon">
      <img src={`/img/vicmojis/vic${style}.svg`} alt="Vic - The Vector Mascot" />
    </div>
    {text && <div className="text">{text}</div>}
  </Link>;
}

export default Vic;
