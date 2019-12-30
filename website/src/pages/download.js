import React from 'react';
import {Redirect} from '@docusaurus/router';

function Download() {
  return <Redirect to="/releases/latest/download/" />;
}

export default Download;
