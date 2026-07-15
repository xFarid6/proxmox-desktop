import pluginVue from "eslint-plugin-vue";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist/", "src-tauri/", "node_modules/", "**/*.d.ts"] },
  tseslint.configs.recommended,
  pluginVue.configs["flat/recommended"],
  {
    files: ["**/*.vue"],
    languageOptions: {
      parserOptions: { parser: tseslint.parser },
    },
  },
);
