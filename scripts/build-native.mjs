import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { nativeBuildEnvironment, NATIVE_TARGET } from './native-build-env.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
if (process.platform !== 'darwin' || process.arch !== 'arm64') throw new Error('Build Sumi on an Apple Silicon Mac.');
const { cargo, environment } = await nativeBuildEnvironment(root);
const test = process.argv.includes('--test');
const check = process.argv.includes('--check');
const command = [test ? 'test' : check ? 'check' : 'build', '--locked', '--offline', '--manifest-path', path.join(root, 'src-native/Cargo.toml'), '--target', NATIVE_TARGET, '--target-dir', environment.CARGO_TARGET_DIR];
if (process.argv.includes('--lib')) command.push('--lib');
if (!test && !check && !process.argv.includes('--debug')) command.push('--release');
execFileSync(cargo, command, { cwd: root, env: environment, stdio: 'inherit' });
