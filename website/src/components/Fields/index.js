import React, {useState} from 'react';

import './styles.css';

function Fields({children, filters}) {
  const [onlyCommon, setOnlyCommon] = useState(false);
  const [onlyRequired, setOnlyRequired] = useState(false);
  const [searchTerm, setSearchTerm] = useState(null);

  let childrenArray = Array.isArray(children) ? children : [children];
  let commonRelevant = childrenArray.some(child => child.props.common);
  let requiredRelevant = childrenArray.some(child => child.props.required);
  let filteredChildren = childrenArray;

  if (onlyCommon) {
    filteredChildren = filteredChildren.filter(child => child.props.common);
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
    <div className="fields">
      {childrenArray.length > 1 && filters !== false ?
        (<div className="filters">
          <span className="result-count">{filteredChildren.length} items</span>
          <div className=" search">
            <input
              type="text"
              onChange={(event) => setSearchTerm(event.currentTarget.value)}
              placeholder="🔍 Search..." />
          </div>
          <div className="checkboxes">
            {commonRelevant && (
              <label title="Only show popular/common results">
              <input
                type="checkbox"
                onChange={(event) => setOnlyCommon(event.currentTarget.checked)}
                checked={onlyCommon} />
              common only
            </label>)}
            {requiredRelevant && (
              <label title="Only show required results">
              <input
                type="checkbox"
                onChange={(event) => setOnlyRequired(event.currentTarget.checked)}
                checked={onlyRequired} />
              required only
            </label>)}
          </div>
        </div>) :
        null}
      <div className="section-list">
        {!Array.isArray(filteredChildren) || filteredChildren.length > 0 ?
          filteredChildren :
          <div className="empty">
            <div className="icon">☹</div>
            <div>No fields found</div>
          </div>}
      </div>
    </div>
  );
}

export default Fields;