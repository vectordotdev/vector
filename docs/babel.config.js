module.exports = function (api) {
  api.cache(true);

  const presets = [
    [
      require('@babel/preset-env'),
      {
        "useBuiltIns": 'entry',
        "corejs": 3
      }
    ],
    [
      require("@babel/preset-react"),
      {
        "flow": false,
        "typescript": true
      }
    ],
    [
      require("@babel/preset-typescript"),
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
