{{ $latest := index site.Data.docs.versions 0 }}
{{ $ddConfig := site.Params.datadog_config }}

import { datadogRum } from '@datadog/browser-rum';
import { datadogLogs } from '@datadog/browser-logs';

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
      applicationId: '{{ $ddConfig.application_id }}',
      clientToken: '{{ $ddConfig.client_token }}',
      env,
      service: '{{ $ddConfig.service_name }}',
      version: '{{ $latest }}',
      trackInteractions: true
    });
  }
}

if (datadogLogs) {
  if (env === 'preview' || env === 'live') {
    datadogLogs.init({
      clientToken: '{{ $ddConfig.client_token }}',
      forwardErrorsToLogs: true,
      env,
      service: '{{ $ddConfig.service_name }}',
      version: '{{ $latest }}'
    })
  }
}