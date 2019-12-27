import _ from 'lodash';
import humanizeString from 'humanize-string';

function groupSortToken(groupName) {
  switch(groupName) {
    case 'breaking change':
      return 'a';
      break;

    case 'feat':
      return 'b';
      break;

    case 'enhancement':
      return 'c';
      break;

    case 'fix':
      return 'd';
      break;

    default:
      return 'e';
      break;
  }
}

export function commitTypeName(groupName) {
  switch(groupName) {
    case 'chore':
      return 'Chore';
      break;

    case 'docs':
      return 'Doc Update';
      break;

    case 'feat':
      return 'New Feature';
      break;

    case 'fix':
      return 'Bug Fix';
      break;

    case 'perf':
      return 'Perf Improvement';
      break;

    default:
      return humanizeString(groupName);
  }
}

export function sortCommitTypes(types) {
  return types.sort((a,b) => groupSortToken(a) > groupSortToken(b));
}

export default {commitTypeName, sortCommitTypes};
