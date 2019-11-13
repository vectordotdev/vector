/**
 * Copyright (c) 2017-present, Facebook, Inc.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */
import * as React from 'react';

const useTheme = () => {
  let defaultTheme = null;

  if (typeof document !== 'undefined')
    defaultTheme = document.querySelector('html').getAttribute('data-theme');

  if (defaultTheme === null && typeof window !== 'undefined')
    defaultTheme = window.localStorage.getItem('theme');

  if (defaultTheme === null && typeof document !== 'undefined') {
    let utcDate = new Date();
    let offset = (new Date().getTimezoneOffset() / 60) * -1;
    let date = new Date(utcDate.getTime() + offset);
    defaultTheme = (date.getHours() >= 18 || date.getHours() < 7 ? 'dark' : null);
  }

  const [theme, setTheme] = React.useState(defaultTheme);

  React.useEffect(() => {
    try {
      setTheme(localStorage.getItem('theme'));
    } catch (err) {
      console.error(err);
    }
  }, [setTheme]);

  const setThemeSyncWithLocalStorage = React.useCallback(
    nextTheme => {
      try {
        localStorage.setItem('theme', nextTheme);
        setTheme(nextTheme);
      } catch (err) {
        console.error(err);
      }
    },
    [setTheme, theme],
  );

  return [theme, setThemeSyncWithLocalStorage];
};

export default useTheme;
