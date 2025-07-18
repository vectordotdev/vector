{{ $latest := index site.Data.docs.versions 0 }}
{{ $ddConfig := site.Params.datadog_config }}
{{ $env := hugo.Environment }}

import { datadogRum } from '@datadog/browser-rum';
import { datadogLogs } from '@datadog/browser-logs';

const env = '{{ $env }}';

if (datadogRum) {
  if (env === 'preview' || env === 'production') {
    datadogRum.init({
      applicationId: '{{ $ddConfig.application_id }}',
      clientToken: '{{ $ddConfig.client_token }}',
      env,
      service: '{{ $ddConfig.service_name }}',
      version: '{{ $latest }}',
      trackInteractions: true,
      internalAnalyticsSubdomain: '8b61d74c'
    });
  }
}

if (datadogLogs) {
  if (env === 'preview' || env === 'production') {
    datadogLogs.init({
      clientToken: '{{ $ddConfig.client_token }}',
      forwardErrorsToLogs: true,
      env,
      service: '{{ $ddConfig.service_name }}',
      version: '{{ $latest }}',
      internalAnalyticsSubdomain: '8b61d74c'
    })
  }
}

