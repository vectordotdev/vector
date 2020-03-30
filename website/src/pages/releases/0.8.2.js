import React from 'react';

import ReleaseNotes from '@site/src/components/ReleaseNotes';

function ReleaseNotesPage() {
  const version = "0.8.2";

  return <ReleaseNotes version={version} />;
}

export default ReleaseNotesPage;
