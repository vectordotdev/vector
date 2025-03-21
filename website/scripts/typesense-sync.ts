import { typesenseSync } from 'typesense-sync';
import { saveSettings } from 'typesense-sync/settings';
import tsConfig from "../typesense.config.json" assert { type: "json" };

const syncCollection = async () => {
  const promises = []

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
