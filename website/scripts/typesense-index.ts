import { synapseStream } from 'synapse-stream';
import * as dotenv from 'dotenv';
const documentsFile = './public/search.json';

dotenv.config();
/*
 * @param {string} documentsFile - Path to the documents file. (required)
 * @param {Object} gitDiff - Object containing updated and deleted files (optional).
 * @param {string} aliasCollectionName - Name of the alias collection (optional).
 */


synapseStream(documentsFile)
  .then(() => {
    console.log('Operation completed successfully with alias.');
  })
  .catch((error) => {
    console.error('An error occurred:', error);
  });
