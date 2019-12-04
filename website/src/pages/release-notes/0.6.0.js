import React from 'react';

import Layout from '@theme/Layout';
import ReleaseNotes from '@site/src/components/ReleaseNotes';
import ReleaseNotesSidebar from '@site/src/components/ReleaseNotesSidebar';

import classnames from 'classnames';

function ReleaseNotesPage() {
  return (
    <Layout title="Vector v0.6.0 Release Notes">
      <main className={classnames('container', 'container--fluid', 'padding-vert--lg')}>
        <ReleaseNotes version="0.6.0" />
      </main>
    </Layout>
  );
}

export default ReleaseNotesPage;