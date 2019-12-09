import React from 'react';

import classnames from 'classnames';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

import './styles.css';

function Avatar({className, id, inline, size, subTitle}) {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {team}} = siteConfig.customFields;
  const member = team.find(member => member.id == id) || team[0];

  if (inline) {
    return (
      <span className="avatar avatar--inline">
        <img
          className={classnames('avatar__photo', `avatar__photo--${size}`)}
          src={member.avatar}
        />
        {member.name}
      </span>
    );
  } else {
    return (
      <div className={classnames('avatar', `avatar--${size}`, className, {'avatar--inline': inline})}>
        <img
          className={classnames('avatar__photo', `avatar__photo--${size}`)}
          src={member.avatar}
        />
        <div className="avatar__intro">
          <div className="avatar__name">{member.name}</div>
          <small className="avatar__subtitle">{subTitle || 'Vector core team'}</small>
        </div>
      </div>
    );
  }
}

export default Avatar;