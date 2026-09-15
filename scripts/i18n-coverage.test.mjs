import { describe, expect, it } from "vitest";
import ts from "typescript";
import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

const root = join(process.cwd(), "src");
const attributes = new Set(["title", "placeholder", "aria-label", "aria-description", "alt", "label", "description", "emptyLabel"]);
// Product identity, language autonyms, and technical protocol labels stay stable.
const literalExceptions = new Set(["Goal", "English", "简体中文", "API", "JSON", "HTTP", "HTTPS", "URL"]);
function files(path) {
  return readdirSync(path, { withFileTypes: true }).flatMap(entry => {
    const full = join(path, entry.name);
    return entry.isDirectory() ? files(full) : /\.tsx?$/.test(entry.name) && !/\.test\./.test(entry.name) ? [full] : [];
  });
}

describe("UI localization boundary", () => {
  it("resolves every static translation call in both bundled languages", () => {
    const missing = [];
    for (const locale of ["en", "zh-CN"]) {
      const directory = join(root, "lib/i18n/locales", locale);
      const catalogs = Object.fromEntries(readdirSync(directory).filter(name => name.endsWith(".json")).map(name => [name.slice(0, -5), JSON.parse(readFileSync(join(directory, name), "utf8"))]));
      const hasKey = (key, catalog) => catalog && (key in catalog || `${key}_other` in catalog);
      for (const file of files(root)) {
        const source = ts.createSourceFile(file, readFileSync(file, "utf8"), ts.ScriptTarget.Latest, true, file.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
        const visit = node => {
          if (ts.isCallExpression(node) && ["t", "translate", "i18n.t"].includes(node.expression.getText(source)) && node.arguments[0] && ts.isStringLiteral(node.arguments[0])) {
            const key = node.arguments[0].text;
            const [namespace, token] = key.split(":");
            if (!(token ? hasKey(token, catalogs[namespace]) : Object.values(catalogs).some(catalog => hasKey(key, catalog)))) {
              missing.push(`${locale} ${relative(root, file)}: ${key}`);
            }
          }
          ts.forEachChild(node, visit);
        };
        visit(source);
      }
    }
    expect(missing).toEqual([]);
  });
  it("keeps visible JSX text and accessibility copy in language catalogs", () => {
    const violations = [];
    for (const file of files(root)) {
      const source = ts.createSourceFile(file, readFileSync(file, "utf8"), ts.ScriptTarget.Latest, true, file.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS);
      const inspect = (node, text) => {
        const normalized = text.replace(/\s+/g, " ").trim();
        if (!/[\p{L}]/u.test(normalized) || literalExceptions.has(normalized)) return;
        const line = source.getLineAndCharacterOfPosition(node.getStart(source)).line + 1;
        violations.push(`${relative(root, file)}:${line}: ${normalized}`);
      };
      const visit = node => {
        if (ts.isJsxText(node)) inspect(node, node.text);
        if (ts.isJsxAttribute(node) && attributes.has(node.name.getText(source)) && node.initializer && ts.isStringLiteral(node.initializer)) {
          inspect(node, node.initializer.text);
        }
        if (ts.isJsxExpression(node) && node.expression && ts.isStringLiteral(node.expression)) inspect(node, node.expression.text);
        ts.forEachChild(node, visit);
      };
      visit(source);
    }
    expect(violations).toEqual([]);
  });
});
