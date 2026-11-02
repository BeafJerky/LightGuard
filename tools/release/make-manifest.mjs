#!/usr/bin/env node
import { mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  DEFAULT_SLUG,
  MIRROR_ORIGIN,
  assetName,
  buildManifest,
  checkManifest,
  mirrorManifest,
  parseArgs,
  releaseUrl,
} from './lib.mjs';

const root = resolve(import.meta.dirname, '..', '..');
const args = parseArgs(process.argv.slice(2));

for (const key of ['version', 'zip', 'sig']) {
  if (!args[key]) throw new Error(`missing --${key}`);
}
if (!args.check && !args.notes) throw new Error('missing --notes');

const version = args.version;
const asset = args.asset ?? assetName(version);
const slug = args.slug ?? DEFAULT_SLUG;
const zip = resolve(args.zip);
const sig = readFileSync(resolve(args.sig), 'utf8').trim();
const size = statSync(zip).size;
const notes = args.notes ? readFileSync(resolve(args.notes), 'utf8').trim() : '';
const pubDate = args.date ?? new Date().toISOString();

const direct = buildManifest({
  version,
  notes,
  pubDate,
  slug,
  asset,
  signature: sig,
  size,
  url: args.url,
});
const out = resolve(args.out ?? resolve(root, 'update'));
const directPath = resolve(out, 'latest.json');
const mirrorPath = resolve(out, 'latest.cn.json');

if (args.check) {
  const expectedDirect = {
    version,
    signature: sig,
    url: args.url ?? releaseUrl(slug, version, asset),
    size,
  };
  const expectedMirror = {
    ...expectedDirect,
    url: `${MIRROR_ORIGIN}${expectedDirect.url}`,
  };
  const problems = [
    ...checkManifest(JSON.parse(readFileSync(directPath, 'utf8')), expectedDirect).map(
      (problem) => `latest.json: ${problem}`,
    ),
    ...checkManifest(JSON.parse(readFileSync(mirrorPath, 'utf8')), expectedMirror).map(
      (problem) => `latest.cn.json: ${problem}`,
    ),
  ];
  if (problems.length > 0) {
    console.error(problems.join('\n'));
    process.exit(1);
  }
  console.log(`manifests match ${version} (${size} bytes, ${zip})`);
} else {
  const mirrored = mirrorManifest(direct);
  mkdirSync(out, { recursive: true });
  writeFileSync(directPath, `${JSON.stringify(direct, null, 2)}\n`);
  writeFileSync(mirrorPath, `${JSON.stringify(mirrored, null, 2)}\n`);
  console.log(`wrote ${directPath} and ${mirrorPath}`);
}
