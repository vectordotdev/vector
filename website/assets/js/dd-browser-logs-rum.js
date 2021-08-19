// if (window.DD_RUM) {
//   window.DD_RUM.init({
//     applicationId: 'test',
//     clientToken: 'test',
//     env,
//     service: 'vector',
//     version: CI_COMMIT_SHORT_SHA,
//     trackInteractions: true,
//     allowedTracingOrigins: [window.location.origin]
//   })
// }

// import { datadogRum } from '@datadog/browser-rum';

// datadogRum.init({
//     applicationId: '0b95923b-b06d-445b-893f-a861e93d6ea3',
//     clientToken: 'puba5f23a97d613091ae2ca8c0f4a135af4',
//     site: 'datadoghq.com',
//     service:'vector',
//     // Specify a version number to identify the deployed version of your application in Datadog 
//     // version: '1.0.0',
//     sampleRate: 100,
//     trackInteractions: true
// });