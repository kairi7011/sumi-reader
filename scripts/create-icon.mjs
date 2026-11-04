import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const png = path.join(root, 'assets/app-icon.png');
const iconset = path.join(root, 'test-artifacts/Sumi.iconset');
await fs.mkdir(iconset, { recursive: true });
execFileSync('/usr/bin/swift', [path.join(root, 'scripts/create-icon.swift'), png], { stdio: 'inherit' });
for (const size of [16, 32, 128, 256, 512]) {
  for (const factor of [1, 2]) {
    const pixels = String(size * factor);
    const filename = `icon_${size}x${size}${factor === 2 ? '@2x' : ''}.png`;
    execFileSync('/usr/bin/sips', ['-z', pixels, pixels, png, '--out', path.join(iconset, filename)], { stdio: 'ignore' });
  }
}
execFileSync('/usr/bin/iconutil', ['-c', 'icns', iconset, '-o', path.join(root, 'assets/app-icon.icns')], { stdio: 'inherit' });
await fs.rm(iconset, { recursive: true });
console.log('Built assets/app-icon.icns');
