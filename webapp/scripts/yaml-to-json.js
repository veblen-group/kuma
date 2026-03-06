const fs = require("fs");
const path = require("path");
const YAML = require("yaml");

const yamlFile = path.join(__dirname, "../kuma.config.yaml");
const jsonFile = path.join(__dirname, "../src/generated/kuma.config.json");

const rawYaml = fs.readFileSync(yamlFile, "utf8");
const parsed = YAML.parse(rawYaml);

fs.mkdirSync(path.dirname(jsonFile), { recursive: true });
fs.writeFileSync(jsonFile, JSON.stringify(parsed, null, 2));

console.log(`Converted ${yamlFile} → ${jsonFile}`);
