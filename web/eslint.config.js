import js from "@eslint/js";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["**/*.{ts,tsx}"],
    languageOptions: {
      globals: globals.browser,
    },
  },
  {
    // Build/codegen scripts run in Node, not the browser.
    files: ["**/*.mjs", "scripts/**/*.js"],
    languageOptions: {
      globals: globals.node,
    },
  },
);
