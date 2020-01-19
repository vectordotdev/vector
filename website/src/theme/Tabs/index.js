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
function SelectSwitcher({placeholder, selectedValue, setSelectedValue, values}) {
  return (
    <Select
      className='react-select-container'
      classNamePrefix='react-select'
      options={values}
      isClearable={false}
      placeholder={placeholder}
      value={values.find(option => option.value == selectedValue)}
      onChange={(selectedOption) => setSelectedValue(selectedOption ? selectedOption.value : null)} />
  );
}

function Tabs(props) {
  const {block, centered, children, defaultValue, placeholder, select, style, values, urlKey} = props;
  const [selectedValue, setSelectedValue] = useState(defaultValue);

  useEffect(() => {
    if (typeof window !== 'undefined' && window.location && urlKey) {
      let queryObj = queryString.parse(window.location.search);

      if (queryObj[urlKey])
        setSelectedValue(queryObj[urlKey]);
    }
  }, []);

  return (
    <>
      <div className="margin-vert--md">
        {values.length > 1 && (select ?
          <SelectSwitcher placeholder={placeholder} selectedValue={selectedValue} setSelectedValue={setSelectedValue} {...props} /> :
          <ListSwitcher selectedValue={selectedValue} setSelectedValue={setSelectedValue} {...props} />)}
      </div>

      {
        Children.toArray(children).filter(
          child => child.props.value === selectedValue,
        )[0]
      }
    </>
  );
}

export default Tabs;
