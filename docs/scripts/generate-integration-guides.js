const fs = require('fs');

const cueJsonOutput = 'data/docs.json';

// Create a directory if it doesn't already exist
const createDir = (dir) => {
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir);
  }
}

// Create the guide's title
const makeTitle = (obj) => {
  var s = `Send ${obj['eventType']}`;

  if (obj['fromService']) {
    s = s.concat(` from ${obj['fromService']}`);
  }

  const to = obj['toService'] || 'anywhere';
  s = s.concat(` to ${to}`);

  return s;
}

// Create the guide's description
const makeDescription = (obj) => {
  var s = `A guide to sending ${obj['eventType']}`;

  if (obj['fromService']) {
    s = s.concat(` from ${obj['fromService']}`);
  }

  const to = obj['toService'] || 'anywhere';
  s = s.concat(` to ${to}`);

  s = s.concat(' in just a few minutes');

  return s;
}

// Create the Markdown string for the guide and write it to the filesystem
const createGuide = (obj) => {
  const title = makeTitle(obj);
  const description = makeDescription(obj);

  var path = `content/en/guides/integrate/${obj['kind']}`;
  createDir(path);

  if (obj['from']) {
    path = path.concat(`/${obj['from']}`);
    createDir(path);
  }

  if (obj['to']) {
    path = path.concat(`/${obj['to']}`);
  }

  const from = obj['componentName'] || obj['from'] || null;
  const to = obj['to'] || null;

  const markdownPath = `${path}.md`;
  const frontMatter = `---
title: ${title}
description: |
  ${description}
from: ${from}
to: ${to}
event_type: ${obj['eventType']}
layout: integrate
domain: integration
kind: ${obj['kind']}
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
      const sourceName = guide['source'];
      const sinkName = guide['sink'];
      const platformName = guide['platform'];
      const serviceName = guide['service'];
      const eventType = guide['event_type'];
      // For cases when the component name differs from the link (e.g. "docker" vs. "docker_logs")
      const componentName = guide['component_name'];

      if (sourceName) {
        const fromService = services[serviceName]['name'];

        // Source only, e.g. /guides/integrate/sources/syslog
        createGuide({
          kind: 'sources',
          from: sourceName,
          eventType: eventType,
          fromService: fromService,
        });

        // Source and sink, e.g. /guides/integrate/sources/syslog/aws_s3
        guide['sinks'].forEach((toSink) => {
          const toService = services[toSink['service']]['name'];

          createGuide({
            kind: 'sources',
            from: sourceName,
            to: toSink['name'],
            eventType: eventType,
            fromService: fromService,
            toService: toService,
          });
        });

      } else if (sinkName) {
        const toService = services[serviceName]['name'];

        // Sink only, e.g. /guides/integrate/sinks/aws_s3
        createGuide({
          kind: 'sinks',
          to: sinkName,
          eventType: eventType,
          toService: toService,
        });
      } else if (platformName) {
        const fromService = services[serviceName]['name'];

        // Platform only, e.g. /guides/integrate/platforms/docker
        createGuide({
          kind: 'platforms',
          from: platformName,
          componentName: componentName,
          eventType: eventType,
          fromService: fromService,
        });

        // Platform and sink, e.g. /guides/integrate/platforms/docker/aws_s3
        guide['sinks'].forEach((toSink) => {
          const toService = services[toSink['service']]['name'];

          createGuide({
            kind: 'platforms',
            from: platformName,
            to: toSink['name'],
            eventType: eventType,
            toService: toService,
            componentName: componentName,
          });
        });
      }
    });
  } catch (err) {
    console.error(err);
  }
}

main();
