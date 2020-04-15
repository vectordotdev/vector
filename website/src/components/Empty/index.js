import React from 'react';

import Link from '@docusaurus/Link';
import Vic from '@site/src/components/Vic';

function Empty({text}) {
  return <section className="empty">
    <Vic style="no" text={text} />
  </section>;
}

export default Empty;
