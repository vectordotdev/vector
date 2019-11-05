import React, {useState} from 'react';

import './styles.css';

function Fields({children, filters}) {
  const [onlyCommon, setOnlyCommon] = useState(false);
  const [searchTerm, setSearchTerm] = useState(null);

  let filteredChildren = children;

  if (onlyCommon) {
    filteredChildren = filteredChildren.filter(child => child.props.common);
  }

  if (searchTerm) {
    filteredChildren = filteredChildren.filter(child =>
      child.props.name.toLowerCase().includes(searchTerm.toLowerCase())
    );
  }

  return (
    <div className="fields">
      {filters !== false ?
        (<div className="filters container">
          <div className="row">
            <div className="col search">
              <i className="feather icon-search"></i>
              <input
                type="text"
                onChange={(event) => setSearchTerm(event.currentTarget.value)}
                placeholder="Search..." />
            </div>
            <div className="col text--right checkboxes">
              <label title="Only show popular/common results">
                common only
                <input
                  type="checkbox"
                  onChange={(event) => setOnlyCommon(event.currentTarget.checked)}
                  checked={onlyCommon} />
              </label>
            </div>
          </div>
        </div>) :
        null}
      <div className="fields-list">
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