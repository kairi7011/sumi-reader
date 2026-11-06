import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { nativeTargetDirectory, NATIVE_TARGET } from './native-build-env.mjs';
import { verifyNativeNotices, verifyNativePackage } from './verify-native-package.mjs';

if (process.platform !== 'darwin' || process.arch !== 'arm64') throw new Error('Package Sumi on an Apple Silicon Mac.');
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const outIndex = process.argv.indexOf('--out');
if (outIndex >= 0 && !process.argv[outIndex + 1]) throw new Error('--out needs a destination ending in Sumi.app.');
const destination = path.resolve(outIndex < 0 ? path.join(root, 'dist/Sumi.app') : process.argv[outIndex + 1]);
if (path.basename(destination) !== 'Sumi.app' || destination === '/Sumi.app') throw new Error('The destination must end in Sumi.app inside a build folder.');
const profile = process.argv.includes('--debug') ? 'debug' : 'release';
const option = (name, fallback) => {
  const index = process.argv.indexOf(name);
  if (index < 0) return fallback;
  if (profile !== 'debug') throw new Error(`${name} is available only with --debug.`);
  const value = process.argv[index + 1];
  if (!value || value.startsWith('--')) throw new Error(`${name} needs a value.`);
  return value;
};
const bundleId = option('--bundle-id', 'com.kairi7011.sumi');
const displayName = option('--display-name', 'Sumi');
if (!/^[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+$/.test(bundleId)) throw new Error('Use a reverse-DNS bundle identifier.');
if (displayName.length > 80 || /[\r\n\x00-\x1f]/.test(displayName)) throw new Error('Use a short single-line display name.');

const binary = path.join(nativeTargetDirectory(root), NATIVE_TARGET, profile, 'sumi-native');
await fs.access(binary);
await verifyNativeNotices();
const manifest = await fs.readFile(path.join(root, 'src-native/Cargo.toml'), 'utf8');
const version = manifest.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
if (!version) throw new Error('Native crate version is missing.');
const staging = `${destination}.staging-${process.pid}`;
const bundle = path.join(staging, 'Sumi.app');
const contents = path.join(bundle, 'Contents');
await fs.mkdir(path.join(contents, 'MacOS'), { recursive: true });
await fs.mkdir(path.join(contents, 'Resources'), { recursive: true });
try {
  const executable = path.join(contents, 'MacOS/Sumi');
  await fs.copyFile(binary, executable);
  await fs.chmod(executable, 0o755);
  if (profile === 'release') execFileSync('/usr/bin/strip', ['-S', '-x', executable], { stdio: 'inherit' });
  await fs.copyFile(path.join(root, 'assets/app-icon.icns'), path.join(contents, 'Resources/sumi.icns'));
  await fs.mkdir(path.join(contents, 'Resources/library'), { recursive: true });
  for (const name of ['sumi_library.py', 'extract.swift']) await fs.copyFile(path.join(root, 'tools/library', name), path.join(contents, 'Resources/library', name));
  await fs.copyFile(path.join(root, 'docs/native-dependency-audit.json'), path.join(contents, 'Resources/native-dependency-audit.json'));
  await fs.cp(path.join(root, 'assets/icons/lucide'), path.join(contents, 'Resources/icons/lucide'), { recursive: true });
  for (const name of ['LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md']) await fs.copyFile(path.join(root, name), path.join(contents, 'Resources', name));
  await fs.cp(path.join(root, 'licenses'), path.join(contents, 'Resources/licenses'), { recursive: true, filter: source => path.basename(source) !== '.gitattributes' });
  const escape = value => String(value).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
  const properties = {
    CFBundleName: displayName, CFBundleDisplayName: displayName, CFBundleExecutable: 'Sumi', CFBundleIdentifier: bundleId,
    CFBundlePackageType: 'APPL', CFBundleInfoDictionaryVersion: '6.0', CFBundleIconFile: 'sumi.icns',
    CFBundleShortVersionString: version, CFBundleVersion: version,
    LSMinimumSystemVersion: '13.0', LSApplicationCategoryType: 'public.app-category.productivity',
    NSHumanReadableCopyright: 'Copyright © 2026 kairi7011', NSPrincipalClass: 'NSApplication',
  };
  const plist = `<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n<plist version="1.0"><dict>\n${Object.entries(properties).map(([key, value]) => `<key>${key}</key><string>${escape(value)}</string>`).join('\n')}\n<key>NSHighResolutionCapable</key><true/>\n<key>NSSupportsAutomaticGraphicsSwitching</key><true/>\n<key>NSRequiresAquaSystemAppearance</key><false/>\n<key>CFBundleDocumentTypes</key><array><dict><key>CFBundleTypeName</key><string>PDF document</string><key>CFBundleTypeRole</key><string>Viewer</string><key>LSHandlerRank</key><string>Alternate</string><key>LSItemContentTypes</key><array><string>com.adobe.pdf</string></array><key>CFBundleTypeExtensions</key><array><string>pdf</string></array></dict></array>\n</dict></plist>\n`;
  await fs.writeFile(path.join(contents, 'Info.plist'), plist);
  await fs.writeFile(path.join(contents, 'PkgInfo'), 'APPL????');
  execFileSync('/usr/bin/plutil', ['-lint', path.join(contents, 'Info.plist')], { stdio: 'inherit' });
  execFileSync('/usr/bin/codesign', ['--force', '--sign', '-', bundle], { stdio: 'inherit' });
  execFileSync('/usr/bin/codesign', ['--verify', '--deep', '--strict', bundle], { stdio: 'inherit' });
  await verifyNativePackage(bundle, { debug: profile === 'debug' });
  await fs.rm(destination, { recursive: true, force: true });
  await fs.rename(bundle, destination);
  console.log(`Built native Sumi ${version}: ${destination}`);
} finally { await fs.rm(staging, { recursive: true, force: true }); }
