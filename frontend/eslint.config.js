import js from '@eslint/js'
import { defineConfig } from 'eslint/config'
import vue from 'eslint-plugin-vue'
import globals from 'globals'
import ts from 'typescript-eslint'

// Flat config follows official ESLint, typescript-eslint and eslint-plugin-vue guides.
// Essential Vue rules detect behavioral errors without rewriting V1 formatting.
export default defineConfig([
  { ignores: ['dist/**', 'node_modules/**', 'playwright-report/**', 'test-results/**'] },
  {
    files: ['**/*.{js,mjs,ts,vue}'],
    extends: [js.configs.recommended],
    languageOptions: { globals: { ...globals.browser, ...globals.node } },
  },
  { files: ['**/*.{ts,vue}'], extends: [ts.configs.recommended] },
  ...vue.configs['flat/essential'],
  { files: ['**/*.vue'], languageOptions: { parserOptions: { parser: ts.parser } } },
])
