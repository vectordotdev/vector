import process from 'node:process';
import { typesenseSync } from 'typesense-sync';
const configFilePath = '../typesense.config.json';

const config = {
  configFilePath,
  shouldSaveSettings: true,
};

typesenseSync(config)
  .then(() => {
    console.log('Typesense sync completed');
  })
  .catch((err) => {
    console.error('An error occurred', err);
    process.exit(1);
  });