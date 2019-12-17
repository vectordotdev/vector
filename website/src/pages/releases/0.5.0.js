import React from 'react';

import Layout from '@theme/Layout';
import ReleaseNotes from '@site/src/components/ReleaseNotes';

function ReleaseNotesPage() {
  const version = "0.5.0";

  return (
    <Layout title={`Vector v${version} Release Notes`} description={`Vector v${version} release notes. Highlights, changes, and updates.`}>
      <main>
        <ReleaseNotes version={version} />
      </main>
    </Layout>
  );
}

export default ReleaseNotesPage;