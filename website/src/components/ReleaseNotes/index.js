import React from 'react';

import Changelog from '@site/src/components/Changelog';
import Heading from '@theme/Heading';

import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

const AnchoredH2 = Heading('h2');

function getRelease(version) {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {releases}} = siteConfig.customFields;

  return releases[version];
}

function ReleaseNotes({version}) {
  const release = getRelease(version);

  return (
    <div className="markdown">
      <AnchoredH2 id="overview">Overview</AnchoredH2>

      <p>
        This is an overview
      </p>

      <AnchoredH2 id="overview">Highlights</AnchoredH2>

      <AnchoredH2 id="overview">Changelog</AnchoredH2>

      <Changelog commits={[]} />
    </div>
  );
}

export default ReleaseNotes;