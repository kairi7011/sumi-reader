import fs from 'node:fs/promises';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const cargoIndex = process.argv.indexOf('--cargo-home');
const cargoHome = cargoIndex >= 0 ? process.argv[cargoIndex + 1] : process.env.CARGO_HOME;
if (!cargoHome) throw new Error('Set CARGO_HOME or pass --cargo-home to the reviewed local Cargo cache. No downloads are performed.');
const digest = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const lockBytes = await fs.readFile(path.join(root, 'src-native/Cargo.lock'));
const manifestBytes = await fs.readFile(path.join(root, 'src-native/Cargo.toml'));
const prior = JSON.parse(await fs.readFile(path.join(root, 'docs/tauri-dependency-audit.json'), 'utf8'));
const oldScripts = JSON.parse(await fs.readFile(path.join(root, 'docs/tauri-build-script-review.json'), 'utf8')).scripts;
const packages = lockBytes.toString().split('[[package]]').slice(1).map(section => Object.fromEntries([...section.matchAll(/^([a-z_]+) = "([^"]*)"$/gm)].map(match => [match[1], match[2]])));
const cache = path.join(cargoHome, 'registry/cache/index.crates.io-1949cf8c6b5b557f');
const sources = path.join(cargoHome, 'registry/src/index.crates.io-1949cf8c6b5b557f');
const results = [];
const scripts = [];
for (const item of packages.filter(item => item.source)) {
  const previous = prior.results.find(checked => checked.name === item.name && checked.version === item.version);
  if (!previous || previous.source !== item.source || previous.checksum !== item.checksum || previous.errors.length) throw new Error(`Unreviewed native dependency: ${item.name}@${item.version}`);
  const archive = await fs.readFile(path.join(cache, `${item.name}-${item.version}.crate`));
  if (digest(archive) !== item.checksum) throw new Error(`Archive integrity mismatch: ${item.name}@${item.version}`);
  const source = path.join(sources, `${item.name}-${item.version}`);
  const manifest = await fs.readFile(path.join(source, 'Cargo.toml'), 'utf8');
  const customBuild = manifest.match(/^build = "([^"]+)"$/m)?.[1];
  if (customBuild) {
    const approved = oldScripts.find(script => script.name === item.name && script.version === item.version && script.script === customBuild);
    const actual = digest(await fs.readFile(path.join(source, customBuild)));
    if (!approved || approved.sha256 !== actual) throw new Error(`Unreviewed build script: ${item.name}@${item.version}`);
    scripts.push(approved);
  }
  results.push({ ...previous, archiveSha256: item.checksum, reusedAudit: 'tauri-dependency-audit.json' });
}
const report = {
  checkedAt: new Date().toISOString(), target: 'aarch64-apple-darwin', minimumAgeHours: 72,
  lockSha256: digest(lockBytes), manifestSha256: digest(manifestBytes), packageCount: results.length,
  priorAudit: 'docs/tauri-dependency-audit.json', priorAdvisoryCheck: prior.checkedAt,
  newDownloads: 0, buildScripts: scripts, results,
};
await fs.writeFile(path.join(root, 'docs/native-dependency-audit.json'), `${JSON.stringify(report, null, 2)}\n`);
console.log(`Verified ${results.length} unchanged packages and ${scripts.length} approved build scripts; zero downloads.`);
