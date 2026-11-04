import { cp, mkdir, rm, copyFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const root = fileURLToPath(new URL('../', import.meta.url));
const destination = path.join(root, 'dist-tauri');
const suffix = process.argv.includes('--debug') ? '' : '.min';
await rm(destination, { recursive: true, force: true });
await mkdir(destination, { recursive: true });
await cp(path.join(root, 'src'), path.join(destination, 'src'), { recursive: true });
const vendor = path.join(destination, 'node_modules/pdfjs-dist');
await mkdir(path.join(vendor, 'legacy/build'), { recursive: true });
for (const file of ['pdf', 'pdf.worker']) {
  await copyFile(path.join(root, `node_modules/pdfjs-dist/legacy/build/${file}${suffix}.mjs`), path.join(vendor, `legacy/build/${file}.mjs`));
}
for (const directory of ['cmaps', 'standard_fonts', 'wasm', 'iccs']) {
  await cp(path.join(root, 'node_modules/pdfjs-dist', directory), path.join(vendor, directory), { recursive: true });
}
await copyFile(path.join(root, 'node_modules/pdfjs-dist/LICENSE'), path.join(vendor, 'LICENSE'));
console.log('Prepared local WebKit frontend.');
