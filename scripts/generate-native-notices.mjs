import fs from 'node:fs/promises';
import path from 'node:path';
import crypto from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { nativeBuildEnvironment, RUST_VERSION } from './native-build-env.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const check = process.argv.includes('--check');
const { sysroot, environment } = await nativeBuildEnvironment(root);
const digest = value => crypto.createHash('sha256').update(value).digest('hex');
const lock = await fs.readFile(path.join(root, 'src-native/Cargo.lock'), 'utf8');
const packages = lock.split('[[package]]').slice(1).map(section => Object.fromEntries([...section.matchAll(/^([a-z_]+) = "([^"]*)"$/gm)].map(match => [match[1], match[2]]))).filter(item => item.source);
const audit = JSON.parse(await fs.readFile(path.join(root, 'docs/native-dependency-audit.json'), 'utf8'));
const fallback = JSON.parse(await fs.readFile(path.join(root, 'licenses/objc2/provenance.json'), 'utf8'));
const cacheRoot = path.join(environment.CARGO_HOME, 'registry/cache');
const registries = await fs.readdir(cacheRoot);
const components = [];
const texts = new Map();
const files = new Map();
const retainText = (label, bytes) => {
  const hash = digest(bytes);
  if (!texts.has(hash)) texts.set(hash, { labels: [], text: bytes.toString('utf8').trimEnd() });
  texts.get(hash).labels.push(label);
  return hash;
};
for (const item of packages) {
  const approved = audit.results.find(entry => entry.name === item.name && entry.version === item.version && entry.checksum === item.checksum && entry.source === item.source);
  if (!approved || approved.errors.length) throw new Error(`Unreviewed package: ${item.name}@${item.version}`);
  const filename = `${item.name}-${item.version}.crate`;
  let archive;
  for (const registry of registries) {
    const candidate = path.join(cacheRoot, registry, filename);
    if (await fs.access(candidate).then(() => true, () => false)) { archive = candidate; break; }
  }
  if (!archive) throw new Error(`Missing reviewed cached archive: ${filename}. No downloads are performed.`);
  if (digest(await fs.readFile(archive)) !== item.checksum) throw new Error(`Archive integrity mismatch: ${filename}`);
  const prefix = `${item.name}-${item.version}/`;
  const entries = execFileSync('/usr/bin/tar', ['-tzf', archive], { encoding: 'utf8', maxBuffer: 8 * 1024 * 1024 }).trim().split('\n');
  const sourceNames = entries.filter(entry => entry.startsWith(prefix) && !entry.slice(prefix.length).includes('/') && /^(?:LICENSE|LICENCE|COPYING|NOTICE)(?:[.-]|$)/iu.test(entry.slice(prefix.length)));
  const licenses = [];
  for (const entry of sourceNames.sort()) {
    const bytes = execFileSync('/usr/bin/tar', ['-xOf', archive, entry], { maxBuffer: 8 * 1024 * 1024 });
    licenses.push({ source: entry.slice(prefix.length), sha256: retainText(`${item.name}@${item.version}: ${entry.slice(prefix.length)}`, bytes) });
  }
  if (!licenses.length) {
    const provenance = fallback.files.find(entry => entry.packages.includes(`${item.name}@${item.version}`));
    if (!provenance) throw new Error(`No reviewed license text for ${item.name}@${item.version}`);
    for (const [name, expected] of [['LICENSE.md', provenance.sha256], ['MIT.txt', fallback.mitTerms.sha256]]) {
      const relative = `licenses/objc2/${name}`;
      const bytes = await fs.readFile(path.join(root, relative));
      if (digest(bytes) !== expected) throw new Error(`License integrity mismatch: ${relative}`);
      licenses.push({ source: relative, sha256: retainText(`${item.name}@${item.version}: upstream ${name}`, bytes) });
    }
  }
  components.push({ name: item.name, version: item.version, source: item.source, checksum: item.checksum, license: approved.license, licenses });
}

for (const name of ['LICENSE', 'FEATHER-LICENSE']) {
  const relative = `assets/icons/lucide/${name}`;
  retainText(`Lucide 0.468.0: ${name}`, await fs.readFile(path.join(root, relative)));
}

const rustDocs = path.join(sysroot, 'share/doc/rust');
files.set('licenses/rust/COPYRIGHT-library.html', await fs.readFile(path.join(rustDocs, 'COPYRIGHT-library.html')));
for (const name of (await fs.readdir(path.join(rustDocs, 'licenses'))).sort()) {
  if (!name.endsWith('.txt')) continue;
  files.set(`licenses/rust/licenses/${name}`, await fs.readFile(path.join(rustDocs, 'licenses', name)));
}

let output = `# Third-party notices\n\nGenerated from the reviewed native Cargo lockfile and checksum-verified cached archives. Regenerate with \`node scripts/generate-native-notices.mjs\`; no dependencies are downloaded or executed.\n\nThis file covers all ${components.length} registry packages in the native dependency graph, including build-time packages, plus the bundled icons. License alternatives are preserved as declared upstream. Sumi's own license is in LICENSE.\n\n## Rust standard library\n\nThe native executable includes the Rust ${RUST_VERSION} standard library. Its unmodified [copyright and dependency notices](licenses/rust/COPYRIGHT-library.html) and referenced license texts are bundled separately. These notices include upstream platform-specific components, which need not all be present in this macOS executable.\n\n## Objective-C bindings\n\nNine objc2-family archives refer to a repository-level license statement but omit it. The exact statement from their recorded source revisions and the standard MIT terms it links to are retained in [licenses/objc2](licenses/objc2/provenance.json). The standard template's copyright placeholder is not a Sumi attribution; no upstream copyright year or owner has been invented.\n\n## Components\n\n| Component | Declared license | License text identifiers |\n| --- | --- | --- |\n`;
for (const item of components) output += `| ${item.name} ${item.version} | ${item.license} | ${item.licenses.map(license => license.sha256.slice(0, 16)).join(', ')} |\n`;
output += '\nLucide 0.468.0: ISC; Feather-derived icons also retain MIT notices. Original SVGs, licenses and immutable source provenance are in `assets/icons/lucide`.\n\n## License texts\n';
for (const [hash, value] of texts) {
  output += `\n### ${hash.slice(0, 16)}\n\n${value.labels.join('; ')}\n\n\`\`\`text\n${value.text}\n\`\`\`\n`;
}
files.set('THIRD_PARTY_NOTICES.md', Buffer.from(output));
const manifest = {
  generator: 'scripts/generate-native-notices.mjs',
  packages: components,
  rustVersion: RUST_VERSION,
  files: Object.fromEntries([...files].map(([name, bytes]) => [name, digest(bytes)])),
};
files.set('licenses/native-manifest.json', Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`));
for (const [relative, bytes] of files) {
  const destination = path.join(root, relative);
  if (check) {
    if (!(await fs.readFile(destination)).equals(bytes)) throw new Error(`Generated notices are stale: ${relative}`);
  } else {
    await fs.mkdir(path.dirname(destination), { recursive: true });
    await fs.writeFile(destination, bytes);
  }
}
console.log(`${check ? 'Verified' : 'Generated'} notices for ${components.length} reviewed crates, Rust ${RUST_VERSION}, and Lucide; zero downloads.`);
