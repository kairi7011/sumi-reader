import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import crypto from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const digest = value => crypto.createHash('sha256').update(value).digest('hex');

export async function verifyNativeNotices() {
  const manifest = JSON.parse(await fs.readFile(path.join(root, 'licenses/native-manifest.json'), 'utf8'));
  const lock = await fs.readFile(path.join(root, 'src-native/Cargo.lock'), 'utf8');
  const packages = lock.split('[[package]]').slice(1).map(section => Object.fromEntries([...section.matchAll(/^([a-z_]+) = "([^"]*)"$/gm)].map(match => [match[1], match[2]]))).filter(item => item.source);
  if (packages.length !== manifest.packages.length || packages.some(item => !manifest.packages.some(known => ['name', 'version', 'source', 'checksum'].every(key => known[key] === item[key])))) throw new Error('Third-party notices do not match the native lockfile. Regenerate them from the reviewed cache.');
  for (const [relative, hash] of Object.entries(manifest.files)) {
    if (path.isAbsolute(relative) || relative.split('/').includes('..')) throw new Error('Invalid license manifest path.');
    if (digest(await fs.readFile(path.join(root, relative))) !== hash) throw new Error(`License file changed: ${relative}`);
  }
  for (const required of ['LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md']) await fs.access(path.join(root, required));
  return manifest;
}

export async function verifyNativePackage(bundle, { debug = false } = {}) {
  const manifest = await verifyNativeNotices();
  const resources = path.join(bundle, 'Contents/Resources');
  for (const required of ['LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md', 'licenses/native-manifest.json', 'licenses/objc2/LICENSE.md', 'licenses/objc2/MIT.txt', 'licenses/objc2/provenance.json', 'icons/lucide/LICENSE', 'icons/lucide/FEATHER-LICENSE']) await fs.access(path.join(resources, required));
  for (const [relative, hash] of Object.entries(manifest.files)) {
    if (digest(await fs.readFile(path.join(resources, relative))) !== hash) throw new Error(`Bundled license file differs: ${relative}`);
  }
  const binary = path.join(bundle, 'Contents/MacOS/Sumi');
  const architecture = execFileSync('/usr/bin/file', ['-b', binary], { encoding: 'utf8' });
  if (!architecture.includes('arm64') || architecture.includes('x86_64')) throw new Error('Expected an ARM64 native executable.');
  const libraries = execFileSync('/usr/bin/otool', ['-L', binary], { encoding: 'utf8' }).trim().split('\n').slice(1).map(line => line.trim().split(' ')[0]);
  if (libraries.some(library => !library.startsWith('/usr/lib/') && !library.startsWith('/System/Library/'))) throw new Error('The executable links a non-system library.');
  if (libraries.some(library => /WebKit|JavaScriptCore|Electron/iu.test(library))) throw new Error('The native reader must not link a browser engine.');
  let files = 0;
  async function inspect(directory) {
    for (const entry of await fs.readdir(directory, { withFileTypes: true })) {
      const filename = path.join(directory, entry.name);
      const relative = path.relative(bundle, filename);
      if (entry.isSymbolicLink()) throw new Error(`Unexpected symlink in bundle: ${relative}`);
      if (entry.isDirectory()) { await inspect(filename); continue; }
      files++;
      if (debug && filename === binary) continue;
      const bytes = await fs.readFile(filename);
      const text = bytes.toString('latin1');
      if (/\/(?:Users|home)\/[^/\s\0]+|[A-Za-z]:\\Users\\/u.test(text) || [os.homedir(), root].some(prefix => bytes.includes(Buffer.from(prefix)))) throw new Error(`Private absolute build path found in ${relative}. Rebuild with source-path remapping.`);
    }
  }
  await inspect(bundle);
  return { files, architecture: 'arm64', linkedLibraries: libraries.length, privatePathScan: debug ? 'release binary scan skipped for debug' : 'passed' };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const bundle = path.resolve(process.argv[2] || path.join(root, 'dist/Sumi.app'));
  const result = await verifyNativePackage(bundle, { debug: process.argv.includes('--debug') });
  console.log(`Verified native bundle: ${result.files} files, ARM64, ${result.linkedLibraries} system libraries; path scan ${result.privatePathScan}.`);
}
