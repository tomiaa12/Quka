import { readFileSync, writeFileSync } from "node:fs";

const raw = process.argv[2] ?? "";
const version = raw.replace(/^v/i, "");
if (!/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(version)) {
  throw new Error(`无效版本号：${raw}`);
}

function rewrite(path, next) {
  writeFileSync(path, next(readFileSync(path, "utf8")));
}

rewrite("package.json", (text) => {
  const json = JSON.parse(text);
  json.version = version;
  return `${JSON.stringify(json, null, 2)}\n`;
});

rewrite("src-tauri/tauri.conf.json", (text) =>
  text.replace(/"version":\s*"[^"]+"/, `"version": "${version}"`),
);

rewrite("src-tauri/Cargo.toml", (text) =>
  text.replace(/^version = "[^"]+"/m, `version = "${version}"`),
);

console.log(`version -> ${version}`);
