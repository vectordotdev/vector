import React, {useState} from 'react';

import CheckboxList from '@site/src/components/CheckboxList';
import Heading from '@theme/Heading';
import Link from '@docusaurus/Link';

import _ from 'lodash';
import {commitTypeName, sortCommitTypes} from '@site/src/exports/commits';
import pluralize from 'pluralize';

const AnchoredH3 = Heading('h3');
const AnchoredH4 = Heading('h4');

function Commit({commit, setSearchTerm}) {
  return (
    <div className="section">
      <div className="badges">
        {commit.breaking_change && <span className="badge badge--danger"><i className="feather icon-alert-triangle"></i> breaking</span>}
        {commit.pr_number && (<span className="badge badge--secondary" style={{minWidth: "65px", textAlign: "center"}}>
          <a href={`https://github.com/timberio/vector/pull/${commit.pr_number}`} target="_blank"><i className="feather icon-git-pull-request"></i> {commit.pr_number}</a>
        </span>)}
        {!commit.pr_number && (<span className="badge badge--secondary" style={{minWidth: "65px", textAlign: "center"}}>
          <a href={`https://github.com/timberio/vector/commit/${commit.sha}`} target="_blank"><i className="feather icon-git-commit"></i> {commit.sha.slice(0,5)}</a>
        </span>)}
      </div>
      <AnchoredH4 id={commit.sha}>
        <span className="badge badge--primary badge--small link" onClick={() => setSearchTerm(commit.scope.name)}>{commit.scope.name}</span>&nbsp;
        {commit.description}
      </AnchoredH4>
    </div>
  );
}

function Commits({commits, groupBy, setSearchTerm}) {
  if (groupBy) {
    const groupedCommits = _(commits).sortBy(commit => commit.scope.name).groupBy(groupBy).value();
    const groupKeys = sortCommitTypes(Object.keys(groupedCommits));

    return(
      <div className="section-list">
        {groupKeys.map((groupKey, catIdx) => (
          <div className="section" key={catIdx}>
            <AnchoredH3 id={groupKey}>{pluralize(commitTypeName(groupKey), groupedCommits[groupKey].length, true)}</AnchoredH3>
            <div className="section-list section-list--compact section-list--hover">
              {groupedCommits[groupKey].map((commit, commitIdx) => (
                <Commit key={commitIdx} commit={commit} setSearchTerm={setSearchTerm} />
              ))}
            </div>
          </div>
        ))}
      </div>
    );
  } else {
    return (
      <div>
        {commits.length}
      </div>
    );
  }
}

function Changelog(props) {
  const {commits} = props;

  const [groupBy, setGroupBy] = useState('type');
  const [onlyTypes, setOnlyTypes] = useState(new Set(['enhancement', 'feat', 'fix', 'perf']));
  const [searchTerm, setSearchTerm] = useState(null);

  let filteredCommits = commits.slice(0);

  if (searchTerm) {
    filteredCommits = filteredCommits.filter(commit => (
      commit.message.toLowerCase().includes(searchTerm.toLowerCase())
    ));
  }

  if (onlyTypes.size > 0) {
    filteredCommits = filteredCommits.filter(commit => onlyTypes.has(commit.type) );

    if (onlyTypes.has("breaking change")) {
      filteredCommits = filteredCommits.filter(commit => commit.breaking_change );
    }
  }

  //
  // Filter Options
  //

  const types = new Set(
    _(commits).
      map(commit => commit.group).
      uniq().
      compact().
      sort().
      value());

  //
  // Render
  //

  return (
    <div>
      {commits.length > 1 ?
        (<div className="filters">
          <div className="search">
            <span className="search--result-count">{filteredCommits.length} items</span>
            <input
              type="text"
              onChange={(event) => setSearchTerm(event.currentTarget.value)}
              placeholder="🔍 Search..."
              className="input--text"
              value={searchTerm || ''} />
          </div>
          <div className="filter">
            <div className="filter--choices">
              <CheckboxList
                values={types}
                currentState={onlyTypes}
                setState={setOnlyTypes} />
            </div>
          </div>
        </div>) :
        null}
      {filteredCommits.length > 0 ?
        <Commits commits={filteredCommits} groupBy={groupBy} setSearchTerm={setSearchTerm} types={types} /> :
        <div className="empty">
          <div className="icon">☹</div>
          <div>No commits found</div>
        </div>}
    </div>
  );
}

export default Changelog;
