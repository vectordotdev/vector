import React, {useState} from 'react';

import CheckboxList from '@site/src/components/CheckboxList';
import Empty from '@site/src/components/Empty';
import Heading from '@theme/Heading';
import Link from '@docusaurus/Link';
import RadioList from '@site/src/components/RadioList';
import Select from 'react-select';

import _ from 'lodash';
import classnames from 'classnames';
import {commitTypeName, sortCommitTypes} from '@site/src/exports/commits';
import pluralize from 'pluralize';
import styles from './styles.module.css';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';

const AnchoredH3 = Heading('h3');
const AnchoredH4 = Heading('h4');
const DEFAULT_TYPES = ['enhancement', 'feat', 'fix', 'perf'];

function ScopeTags({scopes}) {
  return scopes.map((scope, idx) => (
    <span
      key={idx}
      className="badge badge--primary badge--small link"
      onClick={() => setSearchTerm(scope.name)}
      style={{marginRight: "4px"}}
      title={`Filter to '${scope.name}' changes only`}>
      {scope.name}
    </span>
  ));
}

function Commit({commit, setSearchTerm}) {
  return (
    <li>
      <div className="badges">
        {commit.breaking_change && (
          <Link
            to={commit.highlight_permalink}
            className="badge badge--danger"
            title="View upgrade guide...">
            <i className="feather icon-alert-triangle"></i> breaking
          </Link>
        )}
        {!commit.breaking_change && commit.highlight_permalink && (
          <Link
            to={commit.highlight_permalink}
            className="badge badge--warning"
            title="View highlight announcement...">
            <i className="feather icon-gift"></i> highlight
          </Link>
        )}
        {commit.pr_number && (
          <a
            href={`https://github.com/timberio/vector/pull/${commit.pr_number}`}
            target="_blank"
            className="badge badge--secondary"
            style={{minWidth: "65px", textAlign: "center"}}
            title="View pull request...">
            <i className="feather icon-git-pull-request"></i> {commit.pr_number}
          </a>
        )}
        {!commit.pr_number && (
          <a
            href={`https://github.com/timberio/vector/commit/${commit.sha}`}
            target="_blank"
            className="badge badge--secondary"
            style={{minWidth: "65px", textAlign: "center"}}
            title="View commit...">
            <i className="feather icon-git-commit"></i> {commit.sha.slice(0,5)}
          </a>
        )}
      </div>
      <AnchoredH4 id={commit.sha}>
        <ScopeTags scopes={commit.scopes} />
        {commit.description}
      </AnchoredH4>
    </li>
  );
}

function Commits({commits, groupBy, setSearchTerm}) {
  if (groupBy) {
    const groupedCommits = _(commits).sortBy(commit => commit.type).groupBy(groupBy).value();
    const groupKeys = sortCommitTypes(Object.keys(groupedCommits));

    return(
      <ul className="connected-list connected-list--clean">
        {groupKeys.map((groupKey, catIdx) => (
          <li key={catIdx}>
            <AnchoredH3 id={groupKey}>{pluralize(commitTypeName(groupKey), groupedCommits[groupKey].length, true)}</AnchoredH3>
            <ul className="connected-list connected-list--compact connected-list--blend connected-list--hover">
              {groupedCommits[groupKey].map((commit, commitIdx) => (
                <Commit key={commitIdx} commit={commit} setSearchTerm={setSearchTerm} />
              ))}
            </ul>
          </li>
        ))}
      </ul>
    );
  } else {
    return (
      <div>
        {commits.length}
      </div>
    );
  }
}

function Changelog({version}) {
  const context = useDocusaurusContext();
  const {siteConfig = {}} = context;
  const {metadata: {releases}} = siteConfig.customFields;
  const commits = _.flatMap(releases, (release => (
    release.commits.map(commit => {
      commit.version = release.version;
      return commit
    })
  )));

  //
  // State
  //

  const [groupBy, setGroupBy] = useState('type');
  const [onlyTypes, setOnlyTypes] = useState(new Set(DEFAULT_TYPES));
  const [searchTerm, setSearchTerm] = useState(null);
  const [onlyversion, setVersion] = useState(version);

  //
  // Base commits
  //

  let baseCommits = commits.slice(0);

  if (onlyversion) {
    baseCommits = baseCommits.filter(commit => (
      commit.version == onlyversion
    ));
  }

  //
  // Filtered commits
  //

  let filteredCommits = baseCommits;

  if (onlyTypes.size > 0) {
    filteredCommits = filteredCommits.filter(commit => onlyTypes.has(commit.type) );
  }

  if (searchTerm) {
    filteredCommits = filteredCommits.filter(commit => (
      commit.message.toLowerCase().includes(searchTerm.toLowerCase())
    ));
  }

  if (onlyversion) {
    filteredCommits = filteredCommits.filter(commit => (
      commit.version == onlyversion
    ));
  }

  //
  // Filter Options
  //

  const types = new Set(
    _(commits).
      map(commit => commit.type).
      uniq().
      compact().
      sort().
      value());

  //
  // Render
  //

  return (
    <div>
      {baseCommits.length > 5 ?
        (<div className="filters filters--narrow">
          <div className="search">
            <span className="search--result-count">{filteredCommits.length} items</span>
            <input
              type="text"
              onChange={(event) => setSearchTerm(event.currentTarget.value)}
              placeholder="🔍 Search by type, component name, or title..."
              className="input--text input--lg"
              value={searchTerm || ''} />
          </div>
          <div className="filter">
            <div className="filter--choices">
              <CheckboxList
                name="type"
                values={types}
                currentState={onlyTypes}
                setState={setOnlyTypes} />
            </div>
          </div>
        </div>) :
        null}
      {filteredCommits.length > 0 ?
        <Commits
          commits={filteredCommits}
          groupBy={groupBy}
          setSearchTerm={setSearchTerm}
          types={types} /> :
        <Empty text="no commits found" />}
    </div>
  );
}

export default Changelog;
