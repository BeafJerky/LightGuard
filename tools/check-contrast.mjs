import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const PROTO = resolve(import.meta.dirname, '..', '..', 'docs', 'prototype');
const css = [`${PROTO}/styles.css`, `${PROTO}/themes.css`]
  .map((file) => readFileSync(file, 'utf8'))
  .join('\n');

function blockFor(pattern, label) {
  const match = css.match(pattern);
  if (!match) throw new Error(`missing token block: ${label}`);
  return parse(match[1]);
}

function parse(body) {
  const tokens = {};
  for (const part of body.split(';')) {
    const index = part.indexOf(':');
    if (index < 0) continue;
    const name = part.slice(0, index).trim();
    if (!name.startsWith('--')) continue;
    tokens[name] = part.slice(index + 1).trim();
  }
  return tokens;
}

const blocks = {
  'default-dark': blockFor(/:root,\s*\[data-theme="dark"\]\s*\{([\s\S]*?)\}/, 'default-dark'),
  'default-light': blockFor(/\[data-theme="light"\]\s*\{([\s\S]*?)\}/, 'default-light'),
  claude: blockFor(/\[data-theme-pack="claude"\]\s*\{([\s\S]*?)\}/, 'claude'),
  pixel: blockFor(/\[data-theme-pack="pixel"\]\s*\{([\s\S]*?)\}/, 'pixel'),
  nes: blockFor(/\[data-theme-pack="nes"\]\s*\{([\s\S]*?)\}/, 'nes'),
  tech: blockFor(/\[data-theme-pack="tech"\]\s*\{([\s\S]*?)\}/, 'tech'),
};

function parseColor(value) {
  const text = String(value || '').trim();
  const hex = /^#([0-9a-f]{6})$/i.exec(text);
  if (hex) {
    const n = parseInt(hex[1], 16);
    return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255, a: 1 };
  }
  const rgba = /^rgba?\(\s*([\d.]+)\s*,\s*([\d.]+)\s*,\s*([\d.]+)(?:\s*,\s*([\d.]+))?\s*\)$/i.exec(text);
  if (rgba) {
    return {
      r: Number(rgba[1]),
      g: Number(rgba[2]),
      b: Number(rgba[3]),
      a: rgba[4] === undefined ? 1 : Number(rgba[4]),
    };
  }
  throw new Error(`unsupported color: ${text}`);
}

function blend(fg, bg) {
  return {
    r: fg.r * fg.a + bg.r * (1 - fg.a),
    g: fg.g * fg.a + bg.g * (1 - fg.a),
    b: fg.b * fg.a + bg.b * (1 - fg.a),
    a: 1,
  };
}

function luminance(color) {
  const channel = (value) => {
    const c = value / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b);
}

function ratio(fg, bg) {
  const a = luminance(fg);
  const b = luminance(bg);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

function colorOn(tokens, fgKey, bgKey) {
  const bg = parseColor(tokens[bgKey]);
  const fg = parseColor(tokens[fgKey]);
  return { fg: fg.a < 1 ? blend(fg, bg) : fg, bg };
}

const failures = [];
const seen = new Set();

function checkPack(name, tokens, pairs) {
  for (const [fgKey, bgKey, minimum] of pairs) {
    const key = `${name}:${fgKey}/${bgKey}`;
    if (seen.has(key)) continue;
    seen.add(key);
    const { fg, bg } = colorOn(tokens, fgKey, bgKey);
    const value = ratio(fg, bg);
    const ok = value >= minimum;
    console.log(`${ok ? 'PASS' : 'FAIL'} ${name} ${fgKey} on ${bgKey}: ${value.toFixed(2)} (>= ${minimum})`);
    if (!ok) failures.push(`${name} ${fgKey} on ${bgKey}: ${value.toFixed(2)} < ${minimum}`);
  }
}

const textPairs = [
  ['--text', '--bg', 4.5],
  ['--text', '--surface', 4.5],
  ['--text-secondary', '--surface', 4.5],
  ['--text-muted', '--surface', 4.5],
  ['--text-muted', '--surface-2', 4.5],
  ['--text-secondary', '--bg', 4.5],
  ['--text-muted', '--bg', 4.5],
  ['--primary', '--surface', 4.5],
  ['--danger', '--surface', 4.5],
  ['--success', '--surface', 4.5],
  ['--on-primary', '--primary', 4.5],
];

const controlPairs = [
  ['--border-strong', '--surface', 3],
  ['--border-strong', '--surface-2', 3],
  ['--focus', '--surface', 3],
  ['--focus', '--bg', 3],
];

for (const [name, tokens] of Object.entries(blocks)) {
  checkPack(name, tokens, textPairs);
  if (['claude', 'pixel', 'nes', 'tech'].includes(name)) checkPack(name, tokens, controlPairs);
}

if (failures.length) {
  console.error('\nContrast failures:');
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log('\nAll contrast checks passed.');
