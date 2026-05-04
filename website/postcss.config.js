import postcssImport from "postcss-import";
import tailwindCss from "tailwindcss";
import autoprefixer from "autoprefixer";

export default {
  plugins: [
    postcssImport,
    tailwindCss,
    autoprefixer,
  ]
};
