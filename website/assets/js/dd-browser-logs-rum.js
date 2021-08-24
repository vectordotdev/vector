{{ $latest := index site.Data.docs.versions 0 }}

import { ddConfig } from './config-vector';
import { datadogRum } from '@datadog/browser-rum';

console.log('Hello from DD Browser Logs and Rum init JS file.');

// to-do:  make this better.
const getEnv = () => {
  let env;

  if (window.location.hostname.includes('localhost')) {
    env = 'development'
  } else if (window.location.hostname.includes('deploy-preview')) {
    env = 'preview'
  } else {
    env = 'live'
  }

  return env;
}

const env = getEnv();

if (datadogRum) {
  if (env === 'preview' || env === 'live') {
    datadogRum.init({
      applicationId: ddConfig.applicationID,
      clientToken: ddConfig.clientToken,
      env,
      service: 'vector',
      version: '{{ $latest }}',
      trackInteractions: true
    });
  }
}