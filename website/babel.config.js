import presetEnv from "@babel/preset-env";
import presetReact from "@babel/preset-react";
import presetTypeScript from "@babel/preset-typescript";

export default function (api) {
  api.cache(true);

  const presets = [
    presetEnv,
    [
      presetReact,
      {
        runtime: "automatic"
      }
    ],
    presetTypeScript
  ];

  const plugins = [];

  return {
    presets,
    plugins
  };
}
