import React from 'react';

import Link from '@docusaurus/Link';

function Empty({text}) {
  return (
    <section className="empty">
      <Link to="/vic">
        <div className="icon"><img src="/img/vicmojis/vicno.svg" alt="Vic - The Vector Mascot" /></div>
        <div className="text">{text}</div>
      </Link>
    </section>
  );
}

export default Empty;
