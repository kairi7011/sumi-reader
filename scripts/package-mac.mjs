import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

if (process.platform !== 'darwin') throw new Error('Build the macOS app on macOS.');
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const outIndex = process.argv.indexOf('--out');
const destination = path.resolve(outIndex >= 0 ? process.argv[outIndex + 1] : path.join(root, 'dist/Sumi.app'));
if (path.basename(destination) !== 'Sumi.app' || destination === '/Sumi.app') throw new Error('The destination must end in Sumi.app inside a build folder.');
const electron = path.join(root, 'node_modules/electron/dist/Electron.app');
await fs.access(electron);
const packageJson = JSON.parse(await fs.readFile(path.join(root, 'package.json'), 'utf8'));
await fs.mkdir(path.dirname(destination), { recursive: true });
await fs.rm(destination, { recursive: true, force: true });
execFileSync('/usr/bin/ditto', [electron, destination], { stdio: 'inherit' });
const resources = path.join(destination, 'Contents/Resources');
await fs.rm(path.join(resources, 'default_app.asar'), { force: true });
await fs.copyFile(path.join(root, 'assets/app-icon.icns'), path.join(resources, 'sumi.icns'));
for (const license of ['LICENSE', 'LICENSES.chromium.html']) await fs.copyFile(path.join(root, 'node_modules/electron/dist', license), path.join(resources, license));
const appDir = path.join(resources, 'app');
await fs.mkdir(appDir, { recursive: true });
for (const directory of ['electron', 'src', 'assets']) {
  try { await fs.cp(path.join(root, directory), path.join(appDir, directory), { recursive: true }); }
  catch (error) { if (error.code !== 'ENOENT' || directory !== 'assets') throw error; }
}
const pdfjsRoot = path.join(root, 'node_modules/pdfjs-dist');
const pdfjsOut = path.join(appDir, 'node_modules/pdfjs-dist');
await fs.mkdir(pdfjsOut, { recursive: true });
for (const entry of ['build', 'web/pdf_viewer.css', 'web/images', 'cmaps', 'standard_fonts', 'wasm', 'iccs', 'package.json', 'LICENSE']) {
  try { await fs.cp(path.join(pdfjsRoot, entry), path.join(pdfjsOut, entry), { recursive: true }); }
  catch (error) { if (error.code !== 'ENOENT') throw error; }
}
await fs.writeFile(path.join(appDir, 'package.json'), `${JSON.stringify({ name: packageJson.name, productName: 'Sumi', version: packageJson.version, description: packageJson.description, main: 'electron/main.cjs', private: true }, null, 2)}\n`);
await fs.rename(path.join(destination, 'Contents/MacOS/Electron'), path.join(destination, 'Contents/MacOS/Sumi'));
const plist = path.join(destination, 'Contents/Info.plist');
for (const [key, value] of Object.entries({ CFBundleName: 'Sumi', CFBundleDisplayName: 'Sumi', CFBundleExecutable: 'Sumi', CFBundleIdentifier: 'com.kairi7011.sumi', CFBundleIconFile: 'sumi.icns', CFBundleShortVersionString: packageJson.version, CFBundleVersion: packageJson.version, NSHumanReadableCopyright: 'Copyright © 2026 kairi7011' })) {
  execFileSync('/usr/bin/plutil', ['-replace', key, '-string', value, plist]);
}
execFileSync('/usr/bin/plutil', ['-replace', 'CFBundleDocumentTypes', '-json', JSON.stringify([{ CFBundleTypeName: 'PDF document', CFBundleTypeRole: 'Viewer', LSHandlerRank: 'Alternate', LSItemContentTypes: ['com.adobe.pdf'] }]), plist]);
execFileSync('/usr/bin/codesign', ['--force', '--deep', '--sign', '-', destination], { stdio: 'inherit' });
execFileSync('/usr/bin/codesign', ['--verify', '--deep', '--strict', destination], { stdio: 'inherit' });
console.log(`Built ${destination}`);
