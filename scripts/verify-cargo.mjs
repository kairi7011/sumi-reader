import { createHash } from 'node:crypto';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../', import.meta.url));
const lockPath = path.resolve(process.argv[2] || path.join(root, 'src-tauri/Cargo.lock'));
const reportPath = path.resolve(process.argv[3] || path.join(root, '../tauri-dependency-audit.json'));
const cachePath = path.resolve(process.argv[4] || path.join(root, '../crate-metadata'));
const target = process.argv[5] || 'aarch64-apple-darwin';
const lock = await readFile(lockPath, 'utf8');
const digest = value => createHash('sha256').update(value).digest('hex');
const lockSha256 = digest(lock);
const manifestSha256 = digest(await readFile(path.join(path.dirname(lockPath), 'Cargo.toml')));
const packages = lock.split('[[package]]').slice(1).map(block => {
  const get = key => new RegExp(`^${key} = "([^"]+)"`, 'm').exec(block)?.[1];
  return { name: get('name'), version: get('version'), source: get('source'), checksum: get('checksum') };
}).filter(item => item.source);
if (!packages.length) throw new Error('No external Cargo packages found.');
await mkdir(cachePath, { recursive: true });

async function fetchJson(url, init = {}) {
  for (let attempt = 0; attempt < 5; attempt++) {
    const response = await fetch(url, { ...init, headers: { 'User-Agent': 'sumi-reader-dependency-review', ...init.headers }, signal: AbortSignal.timeout(45_000) });
    if ([429, 502, 503, 504].includes(response.status)) {
      await new Promise(resolve => setTimeout(resolve, 1000 * (attempt + 1)));
      continue;
    }
    if (!response.ok) throw new Error(`Metadata request ${response.status}: ${url}`);
    return response.json();
  }
  throw new Error(`Metadata request failed after retries: ${url}`);
}

const metadataCache = new Map();
async function crateMetadata(name) {
  if (metadataCache.has(name)) return metadataCache.get(name);
  const promise = (async () => {
    const filename = path.join(cachePath, `${name}.json`);
    try {
      const cached = JSON.parse(await readFile(filename, 'utf8'));
      if (Date.now() - cached.fetchedAt < 60 * 60 * 1000) return cached.data;
    } catch {}
    const data = await fetchJson(`https://crates.io/api/v1/crates/${encodeURIComponent(name)}`);
    await writeFile(filename, JSON.stringify({ fetchedAt: Date.now(), data }));
    return data;
  })();
  metadataCache.set(name, promise);
  return promise;
}

const results = [];
let cursor = 0;
await Promise.all(Array.from({ length: 6 }, async () => {
  while (cursor < packages.length) {
    const item = packages[cursor++];
    const errors = [];
    try {
      if (item.source !== 'registry+https://github.com/rust-lang/crates.io-index') throw new Error(`Unapproved source: ${item.source}`);
      const metadata = await crateMetadata(item.name);
      const release = metadata.versions.find(version => version.num === item.version);
      if (!release) throw new Error('Exact release missing from authoritative registry.');
      const published = Date.parse(release.created_at);
      if (!Number.isFinite(published)) errors.push('Publication date missing');
      else if (Date.now() - published < 72 * 60 * 60 * 1000) errors.push(`Under age: eligible ${new Date(published + 72 * 60 * 60 * 1000).toISOString()}`);
      if (release.yanked) errors.push('Release is yanked');
      if (!/^[a-f0-9]{64}$/.test(item.checksum || '') || item.checksum !== release.checksum) errors.push('Registry checksum mismatch');
      results.push({ ...item, publishedAt: release.created_at, publisher: release.published_by?.login ?? null, trustedPublishing: release.trustpub_data ?? null, repository: metadata.crate.repository, license: release.license, errors });
    } catch (error) { results.push({ ...item, errors: [error.message] }); }
    if (results.length % 50 === 0) console.log(`Verified metadata for ${results.length}/${packages.length} locked releases`);
  }
}));
results.sort((a, b) => `${a.name}@${a.version}`.localeCompare(`${b.name}@${b.version}`));
const osv = await fetchJson('https://api.osv.dev/v1/querybatch', {
  method: 'POST', headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ queries: packages.map(item => ({ package: { name: item.name, ecosystem: 'crates.io' }, version: item.version })) }),
});
const advisories = osv.results.flatMap((result, index) => (result.vulns || []).map(vuln => ({ name: packages[index].name, version: packages[index].version, id: vuln.id })));
let scope;
try { scope = JSON.parse(await readFile(path.join(root, 'scripts/cargo-advisory-scope.json'), 'utf8')); } catch {}
const scopeMatches = scope?.target === target && scope?.lockSha256 === lockSha256 && scope?.manifestSha256 === manifestSha256;
for (const advisory of advisories) {
  const exclusion = scopeMatches && scope.exclusions.find(item => item.name === advisory.name && item.version === advisory.version && item.ids.includes(advisory.id));
  if (exclusion) advisory.targetExclusion = exclusion.reason;
}
const actionableAdvisories = advisories.filter(item => !item.targetExclusion);
const report = { checkedAt: new Date().toISOString(), target, lockSha256, manifestSha256, minimumAgeHours: 72, packageCount: results.length, results, advisories };
await writeFile(reportPath, JSON.stringify(report, null, 2) + '\n');
for (const item of results.filter(item => item.errors.length)) console.error(`${item.name}@${item.version}: ${item.errors.join('; ')}`);
for (const advisory of advisories) console.error(`${advisory.targetExclusion ? 'TARGET-EXCLUDED' : 'ADVISORY'} ${advisory.name}@${advisory.version}: ${advisory.id}`);
console.log(`${results.filter(item => !item.errors.length).length}/${results.length} packages passed age/source/checksum; ${actionableAdvisories.length} actionable advisories for ${target}. Report: ${reportPath}`);
if (results.some(item => item.errors.length) || actionableAdvisories.length) process.exitCode = 1;
