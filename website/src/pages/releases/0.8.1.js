import React from 'react';

import ReleaseNotes from '@site/src/components/ReleaseNotes';

function ReleaseNotesPage() {
  const version = "0.8.1";

  return <ReleaseNotes version={version} />;
}

export default ReleaseNotesPage;
