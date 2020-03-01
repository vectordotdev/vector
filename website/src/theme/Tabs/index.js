/**
 * Copyright (c) 2017-present, Facebook, Inc.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import React, {useState, useEffect, Children} from 'react';

import Select from 'react-select';

import classnames from 'classnames';
import queryString from 'query-string';

function ListSwitcher({block, centered, className, style, values, selectedValue, setSelectedValue}) {
  return (
    <div className={centered ? "tabs--centered" : ""}>
      <ul
        className={classnames('tabs', className, {
          'tabs--block': block,
        })}
        style={style}
        >
        {values.map(({value, label}) => (
          <li
            className={classnames('tab-item', {
              'tab-item--active': selectedValue === value,
            })}
            key={value}
            onClick={() => setSelectedValue(value)}>
            {label}
          </li>
        ))}
      </ul>
    </div>
  );
}
function SelectSwitcher({selectedValue, setSelectedValue, values}) {
  return (
    <Select
      className='react-select-container'
      classNamePrefix='react-select'
      options={values}
      isClearable={false}
      placeholder="Select a version..."
      value={values.find(option => option.value == selectedValue)}
      onChange={(selectedOption) => setSelectedValue(selectedOption ? selectedOption.value : null)} />
  );
}

function Tabs(props) {
  const {block, centered, children, defaultValue, select, style, values, urlKey} = props;
  const [selectedValue, setSelectedValue] = useState(defaultValue);

  useEffect(() => {
    if (!urlKey) {
      return;
    }

    function loadSelectedValue() {
      if (typeof window !== 'undefined' && window.location) {
        let queryObj = queryString.parse(window.location.search);

        if (queryObj[urlKey])
          setSelectedValue(queryObj[urlKey]);
      }
    }

    loadSelectedValue();
    window.addEventListener('pushstate', loadSelectedValue);

    return () => {
      window.removeEventListener('pushstate', loadSelectedValue);
    }
  }, []);

  function onSelectedValue(selectedValue) {
    if (urlKey) {
      let queryObj = queryString.parse(window.location.search);

      if (queryObj[urlKey] !== selectedValue) {
        queryObj[urlKey] = selectedValue;

        let search = queryString.stringify(queryObj);
        window.history.replaceState(null, null, `${window.location.pathname}?${search}`);
        window.dispatchEvent(new Event('pushstate'));
      }
    }
    return setSelectedValue(selectedValue);
  }

  return (
    <div>
      {values.length > 1 && (select ?
        <SelectSwitcher selectedValue={selectedValue} setSelectedValue={onSelectedValue} {...props} /> :
        <ListSwitcher selectedValue={selectedValue} setSelectedValue={onSelectedValue} {...props} />)}
      <div className="margin-vert--md">
        {
          Children.toArray(children).filter(
            child => child.props.value === selectedValue,
          )[0]
        }
      </div>
    </div>
  );
}

export default Tabs;
