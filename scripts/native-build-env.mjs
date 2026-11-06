import fs from 'node:fs/promises';
import path from 'node:path';
import os from 'node:os';
import { execFileSync } from 'node:child_process';

export const RUST_VERSION = '1.99.1';
export const NATIVE_TARGET = 'aarch64-apple-darwin';

export function nativeTargetDirectory(root, environment = process.env) {
  return path.resolve(root, environment.CARGO_TARGET_DIR || 'src-native/target');
}

export async function localBuildSettings(root) {
  try {
    const value = JSON.parse(await fs.readFile(path.join(root, '.native-toolchain.json'), 'utf8'));
    if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Expected an object.');
    for (const key of ['rustBin', 'cargoHome']) {
      if (value[key] !== undefined && typeof value[key] !== 'string') throw new Error(`${key} must be a path string.`);
    }
    return value;
  } catch (error) {
    if (error.code === 'ENOENT') return {};
    throw new Error(`Cannot read .native-toolchain.json: ${error.message}`);
  }
}

export async function reviewedCargoHome(root) {
  const local = await localBuildSettings(root);
  return path.resolve(root, process.env.CARGO_HOME || local.cargoHome || path.join(os.homedir(), '.cargo'));
}

async function executableOnPath(name, environment) {
  for (const directory of (environment.PATH || '').split(path.delimiter)) {
    if (!directory) continue;
    const candidate = path.resolve(directory, name);
    if (await fs.access(candidate, fs.constants.X_OK).then(() => true, () => false)) return candidate;
  }
}

export async function nativeBuildEnvironment(root) {
  if (process.env.RUSTC_WRAPPER || process.env.RUSTC_WORKSPACE_WRAPPER) throw new Error('Remove unreviewed Rust compiler wrappers before building.');
  const local = await localBuildSettings(root);
  const environment = { ...process.env, MACOSX_DEPLOYMENT_TARGET: '13.0' };
  const explicitBin = process.env.SUMI_RUST_BIN || local.rustBin;
  let cargo;
  let rustc;
  if (explicitBin) {
    const directory = path.resolve(root, explicitBin);
    cargo = path.join(directory, 'cargo');
    rustc = path.join(directory, 'rustc');
  } else {
    const rustup = await executableOnPath('rustup', environment);
    if (rustup) {
      // `which` only locates an already installed toolchain; never invoke an installer.
      const locate = tool => execFileSync(rustup, ['which', '--toolchain', RUST_VERSION, tool], { env: environment, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
      try { cargo = locate('cargo'); rustc = locate('rustc'); }
      catch { throw new Error(`Rust ${RUST_VERSION} is not installed in rustup. Install and review it explicitly, or set SUMI_RUST_BIN and CARGO_HOME. This script does not install tools.`); }
    } else {
      cargo = await executableOnPath('cargo', environment);
      rustc = await executableOnPath('rustc', environment);
    }
  }
  if (!cargo || !rustc) throw new Error(`Rust ${RUST_VERSION} was not found. Set SUMI_RUST_BIN to its bin directory and CARGO_HOME to the reviewed Cargo cache, or provide that installed toolchain on PATH.`);
  environment.RUSTC = rustc;
  environment.PATH = `${path.dirname(cargo)}${path.delimiter}${environment.PATH || ''}`;
  environment.CARGO_HOME = await reviewedCargoHome(root);
  environment.CARGO_TARGET_DIR = nativeTargetDirectory(root);
  const version = execFileSync(rustc, ['--version'], { env: environment, encoding: 'utf8' });
  if (!version.startsWith(`rustc ${RUST_VERSION} `)) throw new Error(`The reviewed build requires Rust ${RUST_VERSION}. Set SUMI_RUST_BIN to that verified installation.`);
  const sysroot = execFileSync(rustc, ['--print', 'sysroot'], { env: environment, encoding: 'utf8' }).trim();
  const flags = environment.CARGO_ENCODED_RUSTFLAGS !== undefined
    ? environment.CARGO_ENCODED_RUSTFLAGS.split('\x1f').filter(Boolean)
    : (environment.RUSTFLAGS || '').split(/\s+/u).filter(Boolean);
  const mappings = [
    [os.homedir(), '/sumi-build/home'],
    [sysroot, '/rust'],
    [environment.CARGO_HOME, '/cargo'],
    [root, '/sumi'],
    [environment.CARGO_TARGET_DIR, '/sumi/target'],
  ];
  // Specific mappings come last so a checkout under the home directory stays anonymous.
  for (const [from, to] of mappings) {
    const aliases = new Set([path.resolve(from), await fs.realpath(from).catch(() => path.resolve(from))]);
    for (const alias of aliases) flags.push(`--remap-path-prefix=${alias}=${to}`);
  }
  delete environment.RUSTFLAGS;
  environment.CARGO_ENCODED_RUSTFLAGS = flags.join('\x1f');
  return { cargo, rustc, sysroot, environment };
}
