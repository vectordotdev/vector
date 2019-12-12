import React from 'react';

import classnames from 'classnames';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

import './styles.css';

function Avatar({className, id, inline, nameSuffix, size, subTitle, vertical}) {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {team}} = siteConfig.customFields;
  const member = team.find(member => member.id == id) || team.find(member => member.id == 'ben');

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
      <div className={classnames('avatar', className, {[`avatar--${size}`]: size, 'avatar--inline': inline, 'avatar--vertical': vertical})}>
        <img
          className={classnames('avatar__photo', `avatar__photo--${size}`)}
          src={member.avatar}
        />
        <div className="avatar__intro">
          <div className="avatar__name">{member.name}{nameSuffix}</div>
          {subTitle !== false && <small className="avatar__subtitle">{subTitle || 'Vector core team'}</small>}
        </div>
      </div>
    );
  }
}

export default Avatar;