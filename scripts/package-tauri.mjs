import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

if (process.platform !== 'darwin' || process.arch !== 'arm64') throw new Error('Build Sumi on an Apple Silicon Mac.');
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const outIndex = process.argv.indexOf('--out');
const destination = path.resolve(outIndex < 0 ? path.join(root, 'dist/Sumi.app') : process.argv[outIndex + 1]);
if (path.basename(destination) !== 'Sumi.app' || destination === '/Sumi.app') throw new Error('The destination must end in Sumi.app inside a build folder.');
const binary = path.join(root, 'src-tauri/target/aarch64-apple-darwin/release/sumi');
await fs.access(binary);
const config = JSON.parse(await fs.readFile(path.join(root, 'src-tauri/tauri.conf.json'), 'utf8'));
await fs.rm(destination, { recursive: true, force: true });
const contents = path.join(destination, 'Contents');
await fs.mkdir(path.join(contents, 'MacOS'), { recursive: true });
await fs.mkdir(path.join(contents, 'Resources'), { recursive: true });
await fs.copyFile(binary, path.join(contents, 'MacOS/Sumi'));
await fs.chmod(path.join(contents, 'MacOS/Sumi'), 0o755);
execFileSync('/usr/bin/strip', ['-S', '-x', path.join(contents, 'MacOS/Sumi')], { stdio: 'inherit' });
await fs.copyFile(path.join(root, 'assets/app-icon.icns'), path.join(contents, 'Resources/sumi.icns'));
await fs.copyFile(path.join(root, 'node_modules/pdfjs-dist/LICENSE'), path.join(contents, 'Resources/PDFjs-LICENSE'));
await fs.cp(path.join(root, 'tools/library'), path.join(contents, 'Resources/library'), { recursive: true, filter: source => !source.includes('__pycache__') && !source.endsWith('.pyc') });
const escape = value => String(value).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
const properties = {
  CFBundleName: 'Sumi', CFBundleDisplayName: 'Sumi', CFBundleExecutable: 'Sumi', CFBundleIdentifier: config.identifier,
  CFBundlePackageType: 'APPL', CFBundleInfoDictionaryVersion: '6.0', CFBundleIconFile: 'sumi.icns',
  CFBundleShortVersionString: config.version, CFBundleVersion: config.version,
  LSMinimumSystemVersion: '13.0', LSApplicationCategoryType: 'public.app-category.productivity',
  NSHumanReadableCopyright: 'Copyright © 2026 kairi7011', NSPrincipalClass: 'NSApplication',
};
const plist = `<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n<plist version="1.0"><dict>\n${Object.entries(properties).map(([key, value]) => `<key>${key}</key><string>${escape(value)}</string>`).join('\n')}\n<key>NSHighResolutionCapable</key><true/>\n<key>NSSupportsAutomaticGraphicsSwitching</key><true/>\n<key>CFBundleDocumentTypes</key><array><dict><key>CFBundleTypeName</key><string>PDF document</string><key>CFBundleTypeRole</key><string>Viewer</string><key>LSHandlerRank</key><string>Alternate</string><key>LSItemContentTypes</key><array><string>com.adobe.pdf</string></array><key>CFBundleTypeExtensions</key><array><string>pdf</string></array></dict></array>\n</dict></plist>\n`;
await fs.writeFile(path.join(contents, 'Info.plist'), plist);
await fs.writeFile(path.join(contents, 'PkgInfo'), 'APPL????');
execFileSync('/usr/bin/codesign', ['--force', '--sign', '-', destination], { stdio: 'inherit' });
execFileSync('/usr/bin/codesign', ['--verify', '--deep', '--strict', destination], { stdio: 'inherit' });
console.log(`Built ${destination}`);
