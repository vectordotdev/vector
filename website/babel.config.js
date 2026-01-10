import presetEnv from '@babel/preset-env';
import presetReact from '@babel/preset-react';
import presetTypeScript from '@babel/preset-typescript';

export default function (api) {
  api.cache(true);

  const presets = [
    [
      presetEnv,
      {
        "useBuiltIns": 'entry',
        "corejs": 3
      }
    ],
    [
      presetReact,
      {
        "flow": false,
        "typescript": true
      }
    ],
    [
      presetTypeScript,
      {
        "isTSX": true,
        "allExtensions": true
      }
    ]
  ];

  const plugins = [];

  return {
    presets,
    plugins
  };
}
