import { createHash } from 'node:crypto';
import { createWriteStream } from 'node:fs';
import { access, mkdir, mkdtemp, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { pipeline } from 'node:stream/promises';
import { Readable, Transform } from 'node:stream';
import path from 'node:path';
import {
  ARCHIVE, DIGEST, DOWNLOAD_URL, EXECUTABLE, PACKAGE_ROOT, VERSION,
  assertPackage, runtimeReady,
} from './electron-runtime.mjs';

const run = promisify(execFile);
const MIN_AGE_MS = 72 * 60 * 60 * 1000;

async function metadata(url) {
  const response = await fetch(url, {
    headers: { Accept: 'application/json', 'User-Agent': 'sumi-reader-runtime-setup' },
    redirect: 'error', signal: AbortSignal.timeout(30_000),
  });
  if (!response.ok) throw new Error(`Metadata request failed (${response.status}): ${url}`);
  return response.json();
}

function requireAge(published, label) {
  const date = Date.parse(published);
  if (!Number.isFinite(date)) throw new Error(`${label}: publication time is missing.`);
  if (Date.now() - date < MIN_AGE_MS) {
    throw new Error(`${label}: eligible after ${new Date(date + MIN_AGE_MS).toISOString()}.`);
  }
}

async function officialDownload(url, redirects = 0) {
  const parsed = new URL(url);
  const hosts = ['github.com', 'release-assets.githubusercontent.com', 'objects.githubusercontent.com'];
  if (parsed.protocol !== 'https:' || !hosts.includes(parsed.hostname) || redirects > 5) {
    throw new Error('Runtime download redirected outside approved HTTPS release hosts.');
  }
  const response = await fetch(parsed, { redirect: 'manual', signal: AbortSignal.timeout(120_000) });
  if ([301, 302, 303, 307, 308].includes(response.status)) {
    const location = response.headers.get('location');
    await response.body?.cancel();
    if (!location) throw new Error('Runtime download redirect is missing its target.');
    return officialDownload(new URL(location, parsed).href, redirects + 1);
  }
  if (!response.ok || !response.body) throw new Error(`Runtime download failed (${response.status}).`);
  return response;
}

async function setup() {
  await assertPackage();
  if (await runtimeReady()) {
    console.log(`Electron ${VERSION} is already installed. No download needed.`);
    return;
  }
  const [registry, release] = await Promise.all([
    metadata('https://registry.npmjs.org/electron'),
    metadata(`https://api.github.com/repos/electron/electron/releases/tags/v${VERSION}`),
  ]);
  const manifest = registry.versions?.[VERSION];
  const repository = typeof manifest?.repository === 'string' ? manifest.repository : manifest?.repository?.url;
  const officialRepository = ['https://github.com/electron/electron', 'git+https://github.com/electron/electron.git'];
  if (manifest?.name !== 'electron' || manifest.version !== VERSION || !manifest.dist?.integrity ||
      !officialRepository.includes(repository) || manifest.deprecated) {
    throw new Error('The authoritative Electron package identity could not be verified.');
  }
  requireAge(registry.time?.[VERSION], `electron@${VERSION}`);
  requireAge(release.published_at, `Electron v${VERSION}`);
  const asset = release.assets?.find(item => item.name === ARCHIVE);
  if (release.tag_name !== `v${VERSION}` || release.draft || release.prerelease ||
      asset?.browser_download_url !== DOWNLOAD_URL || asset?.digest !== `sha256:${DIGEST}`) {
    throw new Error('Official release metadata differs from the reviewed runtime artifact.');
  }
  const dist = path.join(PACKAGE_ROOT, 'dist');
  try {
    await access(dist);
    throw new Error('An incomplete Electron dist already exists. Inspect it and remove it before retrying setup.');
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
  const temporary = await mkdtemp(path.join(PACKAGE_ROOT, '.runtime-'));
  try {
    console.log(`Downloading verified Electron ${VERSION} for Apple Silicon…`);
    const archive = path.join(temporary, ARCHIVE);
    const response = await officialDownload(DOWNLOAD_URL);
    const hash = createHash('sha256');
    let bytes = 0;
    const digestStream = new Transform({
      transform(chunk, encoding, callback) {
        bytes += chunk.length;
        if (bytes > 400 * 1024 * 1024) return callback(new Error('Runtime archive exceeds the allowed size.'));
        hash.update(chunk);
        callback(null, chunk);
      },
    });
    await pipeline(Readable.fromWeb(response.body), digestStream, createWriteStream(archive, { flags: 'wx' }));
    if (hash.digest('hex') !== DIGEST) throw new Error('Runtime SHA-256 mismatch; nothing was extracted.');
    const extracted = path.join(temporary, 'dist');
    await mkdir(extracted);
    await run('/usr/bin/ditto', ['-x', '-k', archive, extracted]);
    const version = (await readFile(path.join(extracted, 'version'), 'utf8')).trim().replace(/^v/, '');
    if (version !== VERSION) throw new Error('Extracted runtime version does not match its pin.');
    await access(path.join(extracted, EXECUTABLE));
    await rename(extracted, dist);
    await writeFile(path.join(PACKAGE_ROOT, 'path.txt'), EXECUTABLE);
    console.log(`Electron ${VERSION} installed; SHA-256 ${DIGEST}.`);
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
}

setup().catch(error => {
  console.error(`Runtime setup failed: ${error.message}`);
  process.exitCode = 1;
});
