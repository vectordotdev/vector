import React, {useState} from 'react';

import CheckboxList from '@site/src/components/CheckboxList';
import Empty from '@site/src/components/Empty';

import _ from 'lodash';

function Fields({children, filters}) {
  const [onlyCommon, setOnlyCommon] = useState(false);
  const [onlyGroups, setOnlyGroups] = useState(new Set());
  const [onlyRequired, setOnlyRequired] = useState(false);
  const [searchTerm, setSearchTerm] = useState(null);

  let childrenArray = [];

  if (children) {
    childrenArray = Array.isArray(children) ? children : [children];
  }

  let commonRelevant = childrenArray.some(child => child.props.common);
  let groups = _(childrenArray).flatMap(child => child.props.groups).uniq().value();
  let requiredRelevant = childrenArray.some(child => child.props.required);
  let filteredChildren = childrenArray;

  if (onlyCommon) {
    filteredChildren = filteredChildren.filter(child => child.props.common);
  }

  if (onlyGroups.size > 0) {
    filteredChildren = filteredChildren.filter(child => Array.from(onlyGroups).every(group => child.props.groups.includes(group)));
  }

  if (onlyRequired) {
    filteredChildren = filteredChildren.filter(child => child.props.required);
  }

  if (searchTerm) {
    filteredChildren = filteredChildren.filter(child =>
      child.props.name.toLowerCase().includes(searchTerm.toLowerCase())
    );
  }

  return (
    <>
      {childrenArray.length > 1 && filters !== false ?
        (<div className="filters">
          <div className="search">
            <span className="search--result-count">{filteredChildren.length} items</span>
            <input
              className="input--text input--lg"
              type="text"
              onChange={(event) => setSearchTerm(event.currentTarget.value)}
              placeholder="🔍 Search..." />
          </div>
          <div className="filter">
            <div className="filter--choices">
              <CheckboxList
                values={groups}
                currentState={onlyGroups}
                setState={setOnlyGroups} />
              {commonRelevant && (
                <label title="Only show popular/common results">
                <input
                  type="checkbox"
                  onChange={(event) => setOnlyCommon(event.currentTarget.checked)}
                  checked={onlyCommon} />
                common
              </label>)}
              {requiredRelevant && (
                <label title="Only show required results">
                <input
                  type="checkbox"
                  onChange={(event) => setOnlyRequired(event.currentTarget.checked)}
                  checked={onlyRequired} />
                required
              </label>)}
            </div>
          </div>
        </div>) :
        null}
      {!Array.isArray(filteredChildren) || filteredChildren.length > 0 ?
        <ul className="connected-list">{filteredChildren}</ul> :
        <Empty text="no components found" />}
    </>
  );
}

export default Fields;
