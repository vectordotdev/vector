const { create } = require('domain');
const fs = require('fs');

const cueJsonOutput = 'data/docs.json';

const createDir = (dir) => {
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir);
  }
}

const createGuide = (info, title, description) => {
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
description: |
  ${description}
from: ${from}
to: ${to}
event_type: ${info['eventType']}
layout: integrate
domain: integration
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
      let title, description;

      const sourceName = guide['source'];
      const sinkName = guide['sink'];
      const platformName = guide['platform'];
      const serviceName = guide['service'];
      const eventType = guide['event_type'];
      // For cases when the component name differs from the link (e.g. "docker" vs. "docker_logs")
      const componentName = guide['component_name'];

      if (sourceName) {
        const fromService = services[serviceName];

        title = `Send ${eventType} from ${fromService['name']} to anywhere`;
        description = `A guide to sending ${eventType} from ${fromService['name']} to anywhere in just a few minutes`;

        // Source only, e.g. /guides/integrate/sources/syslog
        createGuide({
          kind: 'sources',
          from: sourceName,
          eventType: eventType,
        }, title, description);

        // Source and sink, e.g. /guides/integrate/sources/syslog/aws_s3
        guide['sinks'].forEach((toSink) => {
          const toService = services[toSink['service']];

          title = `Send ${eventType} from ${fromService['name']} to ${toService['name']}`;
          description = `A guide to sending ${eventType} from ${fromService['name']} to ${toService['name']} in just a few minutes`;

          createGuide({
            kind: 'sources',
            from: sourceName,
            to: toSink['name'],
            eventType: eventType,
          }, title, description);
        });

      } else if (sinkName) {
        const toService = services[serviceName];

        title = `Send ${eventType} to ${toService['name']}`;
        description = `A guide to sending ${eventType} to ${toService['name']} in just a few minutes`;

        // Sink only, e.g. /guides/integrate/sinks/aws_s3
        createGuide({
          kind: 'sinks',
          to: sinkName,
          eventType: eventType,
        }, title, description);
      } else if (platformName) {
        const fromService = services[serviceName];

        title = `Send ${eventType} from ${fromService['name']} to anywhere`;
        description = `A guide to sending ${eventType} from ${fromService['name']} to anywhere in just a few minutes`;

        // Platform only, e.g. /guides/integrate/platforms/docker
        createGuide({
          kind: 'platforms',
          from: platformName,
          componentName: componentName,
          eventType: eventType,
        }, title, description);

        // Platform and sink, e.g. /guides/integrate/platforms/docker/aws_s3
        guide['sinks'].forEach((toSink) => {
          const toService = services[toSink['service']];

          title = `Send ${eventType} from ${fromService['name']} to ${toService['name']}`;
          description = `A guide to sending ${eventType} from ${fromService['name']} to ${toService['name']} in just a few minutes`;

          createGuide({
            kind: 'platforms',
            from: platformName,
            to: toSink['name'],
            componentName: componentName,
            eventType: eventType,
          }, title, description);
        });
      }
    });
  } catch (err) {
    console.error(err);
  }
}

main();
