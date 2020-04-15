import React from 'react';

import classnames from 'classnames';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

import './styles.css';

function Avatar({bio, className, github, nameSuffix, rel, size, subTitle, vertical}) {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {team}} = siteConfig.customFields;
  const member = team.find(member => member.github == github) || team.find(member => member.id == 'ben');

  return (
    <div className={classnames('avatar', className, {[`avatar--${size}`]: size, 'avatar--vertical': vertical})}>
      <img
        className={classnames('avatar__photo', `avatar__photo--${size}`)}
        src={member.avatar}
      />
      <div className="avatar__intro">
        <div className="avatar__name"><a href={member.github} target="_blank" rel={rel}>{member.name}</a>{nameSuffix}</div>
        {subTitle && <small className="avatar__subtitle">{subTitle}</small>}
        {!subTitle && bio && <small className="avatar__subtitle" dangerouslySetInnerHTML={{__html: member.bio}} />}
      </div>
    </div>
  );
}

export default Avatar;
