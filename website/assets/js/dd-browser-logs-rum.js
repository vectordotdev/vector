// {{ $latest := index site.Data.docs.versions 0 }}
// import { datadogRum } from '@datadog/browser-rum';

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

const getVersion = () => {
  const spruceConfig = localStorage.getItem('__spruce:global');
  const spruceConfigToJSON = JSON.parse(spruceConfig)
  return spruceConfigToJSON["version"]
}

// const test = localStorage.getItem('__spruce:global');
// const parsed = JSON.parse(test)

// console.log(parsed["version"]);


// if (window.DD_RUM) {
//   window.DD_RUM.init({
//     applicationId: 'test',
//     clientToken: 'test',
//     env: getEnv(),
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