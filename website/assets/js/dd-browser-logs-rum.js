{{ $latest := index site.Data.docs.versions 0 }}

import { ddConfig } from './config-vector';
import { datadogRum } from '@datadog/browser-rum';
import { datadogLogs } from '@datadog/browser-logs';

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
      service: ddConfig.service,
      version: '{{ $latest }}',
      trackInteractions: true
    });
  }
}

if (datadogLogs) {
  if (env === 'preview' || env === 'live') {
    datadogLogs.init({
      clientToken: ddConfig.clientToken,
      forwardErrorsToLogs: true,
      env,
      service: ddConfig.service,
      version: '{{ $latest }}'
    })
  }
}