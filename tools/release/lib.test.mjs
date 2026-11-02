import test from 'node:test';
import assert from 'node:assert/strict';
import {
  buildManifest,
  checkManifest,
  mirrorManifest,
  parseArgs,
  releaseUrl,
} from './lib.mjs';

const input = {
  version: '0.14.0',
  notes: 'release notes',
  pubDate: '2026-11-02T00:00:00Z',
  slug: 'BeafJerky/LightGuard',
  asset: 'LightGuard_0.14.0_x64-setup.nsis.zip',
  signature: 'signature-content\n',
  size: 123,
};

test('builds a direct manifest with the release asset URL', () => {
  const manifest = buildManifest(input);
  const platform = manifest.platforms['windows-x86_64'];
  assert.equal(
    platform.url,
    'https://github.com/BeafJerky/LightGuard/releases/download/v0.14.0/LightGuard_0.14.0_x64-setup.nsis.zip',
  );
  assert.equal(platform.signature, 'signature-content');
  assert.equal(platform.size, 123);
  assert.equal(manifest.version, '0.14.0');
});

test('prefixes only the mirror manifest', () => {
  const direct = buildManifest(input);
  const mirrored = mirrorManifest(direct);
  const directUrl = direct.platforms['windows-x86_64'].url;
  const mirrorUrl = mirrored.platforms['windows-x86_64'].url;
  assert.equal(mirrorUrl, `https://gh-proxy.com/${directUrl}`);
  assert.ok(!directUrl.includes('gh-proxy.com'));
});

test('reports version, URL, signature and size mismatches', () => {
  const problems = checkManifest(buildManifest(input), {
    version: '0.15.0',
    signature: 'other',
    url: releaseUrl('BeafJerky/LightGuard', '0.15.0', 'x.zip'),
    size: 1,
  });
  assert.equal(problems.length, 4);
});

test('parses the documented CLI flags', () => {
  assert.deepEqual(parseArgs(['--version', '0.14.0', '--check']), {
    version: '0.14.0',
    check: true,
  });
  assert.throws(() => parseArgs(['--version']), /missing value/);
});
