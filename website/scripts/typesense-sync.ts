const { typesenseSync } = require('typesense-sync');
const { saveSettings } = require('typesense-sync/settings');
const tsConfig = require('../typesense.config.json');

const syncCollection = async () => {
  const promises: Promise<any>[] = []

  for (const collection of tsConfig.collections) {
    console.log(`Updating collection ${collection.name}`)
    promises.push(typesenseSync(collection.name, collection.file_path))
  }

  return await Promise.all(promises)
}

saveSettings()
  .then(() => syncCollection())
  .then(() => console.log('Typesense sync completed'))
  .catch(error => console.log('An error occurred', error))
