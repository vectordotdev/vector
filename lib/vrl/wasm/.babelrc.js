module.exports = {
  plugins: ['@babel/transform-runtime'],
  presets: [
    [
      '@babel/env',
      {
        useBuiltIns: 'usage',
        targets: '> 0.5%, last 1 version, not dead',
      },
    ],
  ],
};
