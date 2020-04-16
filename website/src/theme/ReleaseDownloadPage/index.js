import React from 'react';

import ReleaseDownload from '@site/src/components/ReleaseDownload';

function ReleaseDownloadPage(props) {
  const {content: ReleaseContents} = props;
  const {fontMatter, metadata} = ReleaseContents;
  const {version} = metadata;

  return <ReleaseDownload version={version} />
}

export default ReleaseDownloadPage;
