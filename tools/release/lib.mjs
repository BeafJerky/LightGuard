import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';

export const DEFAULT_SLUG = 'BeafJerky/LightGuard';
export const MIRROR_ORIGIN = 'https://gh-proxy.com/';
export const PLATFORM = 'windows-x86_64';

export function assetName(version) {
  return `LightGuard_${version}_x64-setup.exe`;
}

export function releaseUrl(slug, version, asset = assetName(version)) {
  return `https://github.com/${slug}/releases/download/v${version}/${asset}`;
}

export function buildManifest(input) {
  const url =
    input.url ??
    releaseUrl(input.slug ?? DEFAULT_SLUG, input.version, input.asset ?? assetName(input.version));
  return {
    version: input.version,
    notes: input.notes,
    pub_date: input.pubDate,
    platforms: {
      [PLATFORM]: {
        signature: input.signature.trim(),
        url,
        size: input.size,
      },
    },
  };
}

export function mirrorManifest(manifest, origin = MIRROR_ORIGIN) {
  const platform = manifest.platforms?.[PLATFORM];
  if (!platform) throw new Error(`manifest is missing ${PLATFORM}`);
  return {
    ...manifest,
    platforms: {
      ...manifest.platforms,
      [PLATFORM]: { ...platform, url: `${origin}${platform.url}` },
    },
  };
}

export function checkManifest(manifest, expected) {
  const problems = [];
  if (manifest.version !== expected.version) {
    problems.push(`version: ${manifest.version} != ${expected.version}`);
  }
  const platform = manifest.platforms?.[PLATFORM];
  if (!platform) return [...problems, `missing platform ${PLATFORM}`];
  if (platform.signature !== expected.signature.trim()) {
    problems.push('signature does not match the local .sig file');
  }
  if (platform.url !== expected.url) {
    problems.push(`url: ${platform.url} != ${expected.url}`);
  }
  if (platform.size !== expected.size) {
    problems.push(`size: ${platform.size} != ${expected.size}`);
  }
  return problems;
}

export function sha256File(file) {
  return createHash('sha256').update(readFileSync(file)).digest('hex');
}

export function parseArgs(argv) {
  const args = { check: false };
  for (let index = 0; index < argv.length; index += 1) {
    const token = argv[index];
    if (token === '--check') {
      args.check = true;
      continue;
    }
    if (!token.startsWith('--')) throw new Error(`unknown argument: ${token}`);
    const name = token.slice(2);
    const value = argv[index + 1];
    if (!value || value.startsWith('--')) throw new Error(`missing value for ${token}`);
    args[name] = value;
    index += 1;
  }
  return args;
}
