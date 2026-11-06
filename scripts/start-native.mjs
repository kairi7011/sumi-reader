import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';
import { nativeTargetDirectory, NATIVE_TARGET } from './native-build-env.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const binary = path.join(nativeTargetDirectory(root), NATIVE_TARGET, 'debug/sumi-native');
await fs.access(binary);
const child = spawn(binary, process.argv.slice(2), { cwd: root, stdio: 'inherit', env: process.env });
child.on('error', error => { console.error(error.message); process.exitCode = 1; });
child.on('exit', (code, signal) => { process.exitCode = code ?? (signal === 'SIGINT' ? 130 : 1); });
