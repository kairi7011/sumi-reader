import { constants } from 'node:fs';
import { access, readFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

export const ROOT = fileURLToPath(new URL('../', import.meta.url));
export const VERSION = '45.2.1';
export const ARCHIVE = `electron-v${VERSION}-darwin-arm64.zip`;
export const DIGEST = 'a9a1a0a5b64df714957f83467c84823cb34792205922c6f136550b2403605922';
export const DOWNLOAD_URL = `https://github.com/electron/electron/releases/download/v${VERSION}/${ARCHIVE}`;
export const EXECUTABLE = 'Electron.app/Contents/MacOS/Electron';
export const PACKAGE_ROOT = path.join(ROOT, 'node_modules/electron');
export const BINARY = path.join(PACKAGE_ROOT, 'dist', EXECUTABLE);

export async function assertPackage() {
  if (process.platform !== 'darwin' || process.arch !== 'arm64') {
    throw new Error('This personal build requires an Apple Silicon Mac (darwin-arm64).');
  }
  const project = JSON.parse(await readFile(path.join(ROOT, 'package.json'), 'utf8'));
  const installed = JSON.parse(await readFile(path.join(PACKAGE_ROOT, 'package.json'), 'utf8'));
  const checksums = JSON.parse(await readFile(path.join(PACKAGE_ROOT, 'checksums.json'), 'utf8'));
  if (project.devDependencies?.electron !== VERSION || installed.version !== VERSION || checksums[ARCHIVE] !== DIGEST) {
    throw new Error('Electron package/version/checksum differs from the reviewed runtime pin. Review the dependency update first.');
  }
}

export async function runtimeReady() {
  try {
    const version = await readFile(path.join(PACKAGE_ROOT, 'dist/version'), 'utf8');
    const executable = await readFile(path.join(PACKAGE_ROOT, 'path.txt'), 'utf8');
    await access(BINARY, constants.X_OK);
    return version.trim().replace(/^v/, '') === VERSION && executable.trim() === EXECUTABLE;
  } catch {
    return false;
  }
}
