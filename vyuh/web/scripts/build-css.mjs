import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { basename, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const cssDir = join(root, "public/css");

function removeGenerated() {
  mkdirSync(cssDir, { recursive: true });
  const legacy = /^(landing|docs|console|base)\.css(\.map)?$/;
  const hashed = /^vyuh\.[0-9a-f]{12}\.css$/;

  for (const entry of readdirSync(cssDir)) {
    if (legacy.test(entry) || hashed.test(entry)) {
      rmSync(join(cssDir, entry), { force: true });
    }
  }
}

function compile() {
  execFileSync(
    "sass",
    [
      "--load-path=node_modules",
      "scss/vyuh.scss:public/css/vyuh.css",
      "scss/stylebook.scss:public/css/stylebook.css",
      "--style=expanded",
    ],
    { cwd: root, stdio: "inherit" },
  );
}

function hashCss(name) {
  const source = join(cssDir, name);
  const css = readFileSync(source);
  const hash = createHash("sha256").update(css).digest("hex").slice(0, 12);
  const hashedName = `${basename(name, ".css")}.${hash}.css`;
  copyFileSync(source, join(cssDir, hashedName));
  return hashedName;
}

function writeManifest(entries) {
  const manifest = `${JSON.stringify(entries, null, 2)}\n`;
  writeFileSync(join(cssDir, "manifest.json"), manifest);
}

removeGenerated();
compile();

if (!existsSync(join(cssDir, "vyuh.css"))) {
  throw new Error("Sass did not generate public/css/vyuh.css");
}

writeManifest({
  "vyuh.css": hashCss("vyuh.css"),
});
