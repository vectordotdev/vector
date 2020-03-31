import React, {useState, useEffect, Children} from 'react';

import Select from 'react-select';

import classnames from 'classnames';
import queryString from 'query-string';

function ListSwitcher({block, centered, className, style, values, selectedValue, setSelectedValue}) {
  return (
    <div className={centered ? "tabs--centered" : null}>
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
function SelectSwitcher({placeholder, selectedValue, setSelectedValue, size, values}) {
  let options = values;

  if (options[0].group) {
    let groupedOptions = _.groupBy(options, 'group');

    options = Object.keys(groupedOptions).map(group => {
      return {
        label: group,
        options: groupedOptions[group]
      }
    });
  }

  return (
    <Select
      className={`react-select-container react-select--${size}`}
      classNamePrefix='react-select'
      options={options}
      isClearable={selectedValue}
      placeholder={placeholder}
      value={values.find(option => option.value == selectedValue)}
      onChange={(selectedOption) => setSelectedValue(selectedOption ? selectedOption.value : null)} />
  );
}

function Tabs(props) {
  const {block, centered, children, defaultValue, label, placeholder, select, size, style, values, urlKey} = props;
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
      <div className={`margin-bottom--${size || 'md'}`}>
        {label && <div className="margin-vert--sm">{label}</div>}
        {values.length > 1 && (select ?
          <SelectSwitcher
            placeholder={placeholder}
            selectedValue={selectedValue}
            setSelectedValue={setSelectedValue}
            size={size}
            {...props} /> :
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
