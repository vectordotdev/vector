import React from 'react';

import Layout from '@theme/Layout';
import ReleaseNotes from '@site/src/components/ReleaseNotes';
import ReleaseNotesSidebar from '@site/src/components/ReleaseNotesSidebar';

function ReleaseNotesPage() {
  return (
    <Layout title="Vector v0.6.0 Release Notes">
      <div className="sidebar_">
        <ReleaseNotesSidebar />
      </div>
      <main className="padding-vert--lg">
        <article className="container markdown">
          <header>
            <h1>Vector v0.6.0 Release Notes</h1>
          </header>
          <ReleaseNotes version="0.6.0" />
        </article>
      </main>
    </Layout>
  );
}

export default ReleaseNotesPage;