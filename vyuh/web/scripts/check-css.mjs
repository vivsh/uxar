import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const root = fileURLToPath(new URL("..", import.meta.url));
const failures = [];

function readText(path) {
  return readFileSync(path, "utf8");
}

function walk(dir, files = []) {
  if (!existsSync(dir)) {
    return files;
  }

  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    const stats = statSync(path);
    if (stats.isDirectory()) {
      walk(path, files);
    } else {
      files.push(path);
    }
  }

  return files;
}

function checkNoInlineStyles() {
  const roots = [
    "stylebook",
    "landing",
    "templates/console",
    "../../docs/book/src",
  ];

  for (const dir of roots.map((item) => join(root, item))) {
    for (const file of walk(dir)) {
      if (!/\.(html|md)$/.test(file)) {
        continue;
      }

      if (readText(file).includes("style=")) {
        failures.push(`inline style found in ${relative(root, file)}`);
      }
    }
  }
}

function checkNamespace(cssFile, patterns) {
  const css = readText(join(root, "public/css", cssFile));

  for (const [label, pattern] of patterns) {
    if (pattern.test(css)) {
      failures.push(`${cssFile} contains ${label} selectors`);
    }
  }
}

function reportSizes() {
  for (const cssFile of ["vyuh.css", "stylebook.css"]) {
    const file = join(root, "public/css", cssFile);
    const css = readFileSync(file);
    const gzip = gzipSync(css);
    const rawKb = (css.length / 1024).toFixed(1);
    const gzipKb = (gzip.length / 1024).toFixed(1);
    console.log(`${cssFile}: ${rawKb} KiB raw, ${gzipKb} KiB gzip`);
  }
}

function checkManifest() {
  const manifest = JSON.parse(readText(join(root, "public/css/manifest.json")));
  const hashed = manifest["vyuh.css"];
  if (!/^vyuh\.[0-9a-f]{12}\.css$/.test(hashed ?? "")) {
    failures.push("manifest.json must map vyuh.css to a hashed CSS filename");
    return;
  }

  if (!existsSync(join(root, "public/css", hashed))) {
    failures.push(`manifest hashed CSS file is missing: ${hashed}`);
  }
}

function checkLegacyBundles() {
  for (const cssFile of ["base.css", "landing.css", "docs.css", "console.css"]) {
    if (existsSync(join(root, "public/css", cssFile))) {
      failures.push(`legacy CSS bundle should not be generated: ${cssFile}`);
    }
  }
}

execFileSync("npm", ["run", "build:css"], {
  cwd: root,
  stdio: "inherit",
});

checkNoInlineStyles();
checkNamespace("vyuh.css", [
  ["stylebook", /^\.stylebook-/m],
]);
checkManifest();
checkLegacyBundles();

reportSizes();

if (failures.length > 0) {
  console.error("\nCSS quality check failed:");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log("CSS quality check passed.");
