const { synapseStream } = require('synapse-stream');

const documentsFile = './public/search.json';

// Usage with an alias
const aliasCollectionName = 'vector_test';

/*
 * @param {string} documentsFile - Path to the documents file. (required)
 * @param {Object} gitDiff - Object containing updated and deleted files (optional).
 * @param {string} aliasCollectionName - Name of the alias collection (optional).
 */

synapseStream(documentsFile, {}, aliasCollectionName)
  .then(() => {
    console.log('Operation completed successfully with alias.');
  })
  .catch((error) => {
    console.error('An error occurred:', error);
  });
