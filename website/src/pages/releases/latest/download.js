import React from 'react';
import {Redirect} from '@docusaurus/router';

import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

function LatestDownload() {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {releases}} = siteConfig.customFields;
  const latestRelease = Object.values(releases).reverse()[0];

  return <Redirect to={`/releases/${latestRelease.version}/download/`} />;
}

export default LatestDownload;
