import pluginVue from "eslint-plugin-vue";
import tseslint from "typescript-eslint";

export default tseslint.config(
  // `.claude/` holds agent git worktrees — full checkouts of this repo, each
  // with its own tsconfig. Left unignored, typescript-eslint sees several
  // candidate roots and refuses to parse anything.
  { ignores: ["dist/", "src-tauri/", "node_modules/", ".claude/", "**/*.d.ts"] },
  tseslint.configs.recommended,
  pluginVue.configs["flat/recommended"],
  {
    files: ["**/*.vue"],
    languageOptions: {
      parserOptions: { parser: tseslint.parser },
    },
  },
);
