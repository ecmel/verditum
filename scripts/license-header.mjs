// Refresh source headers and LICENSE from one copyright configuration.
// Adapted from the license-header mechanism in Projects/nb.
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const { holder, start } = JSON.parse(
  readFileSync(`${root}copyright.json`, "utf8"),
);
const year = new Date().getFullYear();
const notice = `Copyright (c) ${start}-${year} ${holder}`;
const LICENSE_LINES = [
  notice,
  "See LICENSE in the project root for license information.",
];

const renderHeader = () => {
  const body = LICENSE_LINES.map((line) => ` * ${line}`).join("\n");
  return `/*\n${body}\n */\n\n`;
};

// Replace the entire leading copyright block so wording and years can change.
const STRIP_PATTERN =
  /^\/\*\n \* Copyright \(c\) [^\n]*\n(?: \*[^\n]*\n)*? \*\/\n\n/;

const update = (file, original, updated) => {
  if (updated === original) return;
  writeFileSync(`${root}${file}`, updated);
  console.log(`updated: ${file}`);
};

const sources = (dir, ext) =>
  readdirSync(`${root}${dir}`, { withFileTypes: true })
    .sort((a, b) => a.name.localeCompare(b.name))
    .flatMap((entry) => {
      const file = `${dir}/${entry.name}`;
      if (entry.isDirectory()) return sources(file, ext);
      return entry.isFile() && entry.name.endsWith(ext) ? [file] : [];
    });

for (const file of [
  ...sources("src", ".ts"),
  ...sources("src-tauri/src", ".rs"),
]) {
  const original = readFileSync(`${root}${file}`, "utf8");
  update(file, original, renderHeader() + original.replace(STRIP_PATTERN, ""));
}

const license = readFileSync(`${root}LICENSE`, "utf8");
update(
  "LICENSE",
  license,
  license.replace(
    /^( *)Copyright \(c\) [^\n]*/m,
    (_, indent) => `${indent}${notice}`,
  ),
);
