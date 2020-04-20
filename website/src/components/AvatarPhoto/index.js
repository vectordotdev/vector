import React from 'react';

import classnames from 'classnames';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

function AvatarPhoto({className, github, size}) {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {team}} = siteConfig.customFields;
  const member = team.find(member => member.github == github) || team.find(member => member.id == 'ben');

  return (
    <img
      className={classnames('avatar__photo', `avatar__photo--${size}`, className)}
      src={member.avatar}/>
  );
}

export default AvatarPhoto;
