const { create } = require('domain');
const fs = require('fs');

const cueJsonOutput = 'data/docs.json';

const createDir = (dir) => {
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir);
  }
}

const createGuide = (info, title) => {
  var path = `content/en/guides/integrate/${info['kind']}`;
  createDir(path);

  if (info['from']) {
    path = path.concat(`/${info['from']}`);
    createDir(path);
  }

  if (info['to']) {
    path = path.concat(`/${info['to']}`);
  }

  const from = info['componentName'] || info['from'] || null;
  const to = info['to'] || null;

  const markdownPath = `${path}.md`;
  const frontMatter = `---
title: ${title}
from: ${from}
to: ${to}
event_type: ${info['eventType']}
layout: integrate
kind: ${info['kind']}
---`;

  fs.writeFileSync(markdownPath, frontMatter, 'utf8');
}

const main = () => {
  try {
    const data = fs.readFileSync(cueJsonOutput, 'utf8');
    const docs = JSON.parse(data);
    const guides = docs['guides']['integrate'];
    const services = docs['services'];

    ['sources', 'sinks', 'platforms'].forEach((kind) => {
      const dir = `content/en/guides/integrate/${kind}`;
      createDir(dir);
    });

    guides.forEach((guide) => {
      let title;

      const source = guide['source'];
      const sink = guide['sink'];
      const platform = guide['platform'];
      const service = guide['service'];
      const eventType = guide['event_type'];
      const componentName = guide['component_name'];

      if (source) {
        const fromService = services[service];

        title = `Send ${eventType} from ${fromService['name']} to anywhere`;
        createGuide({
          kind: 'sources',
          from: source,
          eventType: eventType,
        }, title);

        guide['sinks'].forEach((toSink) => {
          const toService = services[toSink['service']];
          title = `Send ${eventType} from ${fromService['name']} to ${toService['name']}`;
          createGuide({
            kind: 'sources',
            from: source,
            to: toSink['name'],
            eventType: eventType,
          }, title);
        });
      } else if (sink) {
        const toService = services[service];
        title = `Send ${eventType} to ${toService['name']}`;
        createGuide({
          kind: 'sinks',
          to: sink,
          eventType: eventType,
        }, title);
      } else if (platform) {
        const fromService = services[service];

        title = `Send ${eventType} from ${fromService['name']} to anywhere`;
        createGuide({
          kind: 'platforms',
          from: platform,
          componentName: componentName,
          eventType: eventType,
        }, title);

        guide['sinks'].forEach((toSink) => {
          const toService = services[toSink['service']];
          title = `Send ${eventType} from ${fromService['name']} to ${toService['name']}`;
          createGuide({
            kind: 'platforms',
            from: platform,
            to: toSink['name'],
            componentName: componentName,
            eventType: eventType,
          }, title);
        });
      }
    });
  } catch (err) {
    console.error(err);
  }
}

main();
