/**
 * Copyright (c) 2017-present, Facebook, Inc.
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */
import * as React from 'react';

const useTheme = () => {
  let defaultTheme = null;

  // Make sure we're in the browser / client context
  if (typeof document !== 'undefined' && typeof window !== 'undefined') {
    defaultTheme = document.querySelector('html').getAttribute('data-theme');

    // Reset the theme if it is a null like value
    if (defaultTheme == '' || defaultTheme == 'null') {
      defaultTheme = null;
    }

    if (defaultTheme === null)
      defaultTheme = window.localStorage.getItem('theme');

    if (defaultTheme === null && window.matchMedia("(prefers-color-scheme: dark)").matches) {
      defaultTheme = 'dark';
    }

    if (defaultTheme === null && window.matchMedia("(prefers-color-scheme: light)").matches) {
      defaultTheme = '';
    }

    if (defaultTheme === null) {
      let utcDate = new Date();
      let offset = (new Date().getTimezoneOffset() / 60) * -1;
      let date = new Date(utcDate.getTime() + offset);
      defaultTheme = (date.getHours() >= 18 || date.getHours() < 7 ? 'dark' : null);
    }
  }

  const [theme, setTheme] = React.useState(defaultTheme);

  // React.useEffect(() => {
  //   try {
  //     setTheme(localStorage.getItem('theme'));
  //   } catch (err) {
  //     console.error(err);
  //   }
  // }, [setTheme]);

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
