import { readFile, writeFile } from "node:fs/promises";

const required = ["VERSION", "TARGET_ABI", "SOURCE_URL", "CHECKSUM"];
const missing = required.filter((name) => !process.env[name]);
if (missing.length) {
  console.error(`Missing environment variables: ${missing.join(", ")}`);
  process.exit(1);
}

const replacements = {
  VERSION: process.env.VERSION,
  TARGET_ABI: process.env.TARGET_ABI,
  SOURCE_URL: process.env.SOURCE_URL,
  CHECKSUM: process.env.CHECKSUM,
  CHANGELOG: process.env.CHANGELOG ?? "See the project release notes.",
  TIMESTAMP: process.env.TIMESTAMP ?? new Date().toISOString()
};
let template = await readFile(new URL("manifest.template.json", import.meta.url), "utf8");
for (const [name, value] of Object.entries(replacements)) {
  template = template.replaceAll(`{{${name}}}`, JSON.stringify(value).slice(1, -1));
}
JSON.parse(template);
await writeFile(new URL("manifest.json", import.meta.url), template);
console.log("Wrote packaging/catalog/manifest.json");
