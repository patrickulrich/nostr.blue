import js from "@eslint/js";
import globals from "globals";
import tseslint from "typescript-eslint";
import svelteEslint from "eslint-plugin-svelte";
import htmlEslint from "@html-eslint/eslint-plugin";
import htmlParser from "@html-eslint/parser";
import customRules from "./eslint-rules/index.js";

export default tseslint.config(
  { ignores: ["dist", ".svelte-kit", "build"] },
  {
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    files: ["**/*.{ts,js}"],
    languageOptions: {
      ecmaVersion: 2020,
      globals: globals.browser,
    },
    linterOptions: {
      noInlineConfig: true, // Prevents all eslint-disable comments
      reportUnusedDisableDirectives: "off", // Don't report unused disable directives (they're in generated code)
    },
    plugins: {
      "custom": customRules,
    },
    rules: {
      "@typescript-eslint/no-unused-vars": [
        "error",
        {
          "argsIgnorePattern": "^_",
          "varsIgnorePattern": "^_",
          "ignoreRestSiblings": true,
        },
      ],
      "custom/no-placeholder-comments": "error",
      "no-warning-comments": [
        "error",
        { terms: ["fixme"] },
      ],
    },
  },
  ...svelteEslint.configs["flat/recommended"],
  {
    files: ["**/*.svelte"],
    languageOptions: {
      parserOptions: {
        parser: tseslint.parser,
      },
    },
    linterOptions: {
      reportUnusedDisableDirectives: "off",
    },
    plugins: {
      "custom": customRules,
    },
    rules: {
      "custom/no-placeholder-comments": "error",
      // Disable $props() custom element warnings for shadcn-svelte components
      "svelte/valid-compile": ["error", { ignoreWarnings: true }],
    },
  },
  {
    // Allow 'any' type in specific files where it's necessary for thunks and signers
    files: ["src/lib/stores/following.svelte.ts", "src/lib/stores/messages.svelte.ts", "src/lib/components/ConversationThread.svelte"],
    rules: {
      "@typescript-eslint/no-explicit-any": "off",
    },
  },
  {
    files: ["**/*.html"],
    plugins: {
      "@html-eslint": htmlEslint,
      "custom": customRules,
    },
    languageOptions: {
      parser: htmlParser,
    },
    rules: {
      "@html-eslint/require-title": "error",
      "@html-eslint/require-meta-charset": "error",
      "@html-eslint/require-meta-description": "error",
      "@html-eslint/require-meta-viewport": "error",
      "@html-eslint/require-open-graph-protocol": [
        "error",
        [
          "og:type",
          "og:title",
          "og:description",
        ],
      ],
      "custom/no-inline-script": "error",
      "custom/require-webmanifest": "error",
    },
  },
  {
    // Allow inline scripts in app.html for theme prevention
    files: ["src/app.html"],
    rules: {
      "custom/no-inline-script": "off",
    },
  }
);
