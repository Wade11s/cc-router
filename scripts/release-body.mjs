#!/usr/bin/env node
// 由 release-notes/<版本>/ 生成 GitHub Release 正文 (spec §3.3)。
// 用法:
//   node scripts/release-body.mjs v6.1.0            正文输出到 stdout
//   node scripts/release-body.mjs --check v6.1.0    只检查 zh.md 是否写好, 不输出正文
import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const args = process.argv.slice(2);
const check = args[0] === "--check";
const tag = check ? args[1] : args[0];
if (!tag) {
  console.error("用法: node scripts/release-body.mjs [--check] <tag|版本>");
  process.exit(2);
}
const version = tag.replace(/^v/, "");
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const dir = resolve(root, "release-notes", version);

function read(name) {
  const p = resolve(dir, name);
  return existsSync(p) ? readFileSync(p, "utf8").replace(/\r\n/g, "\n").trim() : null;
}

const zh = read("zh.md");
// version:set 生成的骨架只有分节标题 —— 至少要有一条 `- ` 列表项才算写过
if (!zh || !/^- \S/m.test(zh)) {
  console.error(`release-notes/${version}/zh.md 缺失或还没写内容 (至少需要一条 \`- \` 列表项)。发版前先写好更新内容。`);
  process.exit(1);
}
if (check) {
  console.error(`release-notes/${version}/zh.md 已就绪`);
  process.exit(0);
}

const metaRaw = read("meta.json");
const meta = metaRaw ? JSON.parse(metaRaw) : {};
const blocks = [`# ${meta.codename ? `v${version} · ${meta.codename}` : `v${version}`}`];
for (const [file, label] of [["zh.md", "中文"], ["en.md", "English"], ["ja.md", "日本語"]]) {
  const text = file === "zh.md" ? zh : read(file);
  if (text) blocks.push(`**${label}**\n\n${text}`);
}
process.stdout.write(blocks.join("\n\n---\n\n") + "\n");
