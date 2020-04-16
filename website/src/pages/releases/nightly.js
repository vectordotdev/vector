import React from 'react';
import {Redirect} from '@docusaurus/router';

function Nightly() {
  return <Redirect to={`/releases/nightly/download/`} />;
}

export default Nightly;
