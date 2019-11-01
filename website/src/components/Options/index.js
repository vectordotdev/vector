import React, {useState} from 'react';

import './styles.css';

function Options({children, filters}) {
  const [onlySimple, setOnlySimple] = useState(false);
  const [searchTerm, setSearchTerm] = useState(null);

  let filteredChildren = children;

  if (onlySimple) {
    filteredChildren = filteredChildren.filter(child => child.props.simple);
  }

  if (searchTerm) {
    filteredChildren = filteredChildren.filter(child =>
      child.props.name.toLowerCase().includes(searchTerm.toLowerCase())
    );
  }

  console.log(filteredChildren)

  return (
    <div className="options">
      {filters ?
        (<div className="filters ">
          <div className="search">
            <i className="feather icon-search"></i>
            <input
              type="search"
              onChange={(event) => setSearchTerm(event.currentTarget.value)}
              placeholder="Search options..." />
          </div>
          <div className="text--right checkboxes">
            <label title="Simple options are popular/common options to simplify getting started">
              simple options only
              <input
                type="checkbox"
                onChange={(event) => setOnlySimple(event.currentTarget.checked)}
                checked={onlySimple} />
            </label>
          </div>
        </div>) :
        null}
      <div className="options-list">
        {!Array.isArray(filteredChildren) || filteredChildren.length > 0 ?
          filteredChildren :
          <div className="empty">
            <div className="icon">☹</div>
            <div>No options found</div>
          </div>}
      </div>
    </div>
  );
}

export default Options;