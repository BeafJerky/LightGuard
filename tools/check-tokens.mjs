import { readFileSync, statSync } from 'node:fs';
import { resolve } from 'node:path';

const PROTO = resolve(import.meta.dirname, '..', '..', 'docs', 'prototype');
const read = (file) => readFileSync(file, 'utf8');
const styles = read(`${PROTO}/styles.css`);
const desktop = read(`${PROTO}/desktop.css`);
const themes = read(`${PROTO}/themes.css`);
const components = `${styles}\n${desktop}`;

const failures = [];

for (const file of [`${PROTO}/styles.css`, `${PROTO}/desktop.css`, `${PROTO}/themes.css`]) {
  const css = read(file);
  if (/@import\b/.test(css)) failures.push(`${file}: @import is not allowed`);
  if (/url\(\s*['"]?https?:/i.test(css)) failures.push(`${file}: external url()`);
  if (/url\(\s*['"]?\/\//i.test(css)) failures.push(`${file}: protocol-relative url()`);
  if (/https?:\/\//i.test(css)) failures.push(`${file}: external URL`);
}

const themesSize = statSync(`${PROTO}/themes.css`).size;
console.log(`themes.css: ${themesSize} bytes`);
if (themesSize > 30 * 1024) failures.push(`themes.css exceeds 30KB (${themesSize})`);

/* Brace-balanced extraction: a greedy regex would swallow component rules and
   make the leakage comparison meaningless. Only the declared block is read. */
function blockBody(css, header, label) {
  const match = header.exec(css);
  if (!match) throw new Error(`missing token block: ${label}`);
  const open = css.indexOf('{', match.index);
  let depth = 0;
  for (let i = open; i < css.length; i += 1) {
    if (css[i] === '{') depth += 1;
    else if (css[i] === '}') {
      depth -= 1;
      if (depth === 0) return css.slice(open + 1, i);
    }
  }
  throw new Error(`unterminated token block: ${label}`);
}

const COLOR_RE = /#[0-9a-f]{3,8}\b|rgba?\([^)]*\)/gi;

function canonical(color) {
  const text = color.toLowerCase().replace(/\s+/g, '');
  const hex = /^#([0-9a-f]{3,8})$/.exec(text);
  if (hex) {
    let value = hex[1];
    if (value.length === 3 || value.length === 4) value = value.split('').map((c) => c + c).join('');
    if (value.length === 6) value += 'ff';
    return `#${value}`;
  }
  const rgb = /^rgba?\(([\d.]+),([\d.]+),([\d.]+)(?:,([\d.]+))?\)$/.exec(text);
  if (!rgb) return text;
  const alpha = rgb[4] === undefined ? 1 : Number(rgb[4]);
  return `rgba(${Number(rgb[1])},${Number(rgb[2])},${Number(rgb[3])},${alpha})`;
}

const defaultBodies = [
  blockBody(styles, /(?:^|\n)\s*:root,\s*\[data-theme="dark"\]\s*\{/, 'default-dark'),
  blockBody(styles, /(?:^|\n)\s*\[data-theme="light"\]\s*\{/, 'default-light'),
];

const defaultColors = new Set();
for (const body of defaultBodies) {
  for (const color of body.match(COLOR_RE) || []) defaultColors.add(canonical(color));
}
console.log(`default token colors: ${defaultColors.size}`);

const componentColors = new Set();
for (const color of components.match(COLOR_RE) || []) componentColors.add(canonical(color));

function packBlock(name) {
  return blockBody(themes, new RegExp(`(?:^|\\n)\\[data-theme-pack="${name}"\\]\\s*\\{`), name);
}

for (const name of ['claude', 'pixel', 'nes', 'tech']) {
  const colors = new Set((packBlock(name).match(COLOR_RE) || []).map(canonical));
  const candidates = [...colors].filter((color) => !defaultColors.has(color));
  const leaked = candidates.filter((color) => componentColors.has(color));
  console.log(`${name}: ${colors.size} colors, ${candidates.length} non-default, ${leaked.length} leaked`);
  for (const color of leaked) {
    failures.push(`${name}: pack color ${color} leaked into styles.css/desktop.css`);
  }
}

if (/url\(\s*['"]?\.\/fonts\/[^)]+\)/.test(themes) === false) {
  failures.push('themes.css: missing local LG Pixel font');
}

if (failures.length) {
  console.error('Token checks failed:');
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log('All token checks passed.');
