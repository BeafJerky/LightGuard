function parseRgb(color: string): [number, number, number] | null {
  const value = color.trim();
  if (value.startsWith('#')) {
    const raw = value.slice(1);
    const full = raw.length === 3 ? raw.split('').map((char) => char + char).join('') : raw;
    if (full.length !== 6) return null;
    const int = Number.parseInt(full, 16);
    if (Number.isNaN(int)) return null;
    return [(int >> 16) & 255, (int >> 8) & 255, int & 255];
  }
  const rgb = value.match(/rgba?\(([^)]+)\)/);
  if (rgb) {
    const parts = rgb[1].split(',').map((part) => Number.parseFloat(part));
    if (parts.length >= 3 && parts.slice(0, 3).every((part) => Number.isFinite(part))) {
      return [parts[0], parts[1], parts[2]];
    }
  }
  return null;
}

export function withAlpha(color: string, alpha: number): string {
  const rgb = parseRgb(color);
  if (!rgb) return color;
  return `rgba(${rgb[0]}, ${rgb[1]}, ${rgb[2]}, ${alpha})`;
}

export function mixColor(color: string, target: string, ratio: number): string {
  const from = parseRgb(color);
  const to = parseRgb(target);
  if (!from || !to) return color;
  const weight = Math.min(1, Math.max(0, ratio));
  const channel = (index: number) =>
    Math.round(from[index] + (to[index] - from[index]) * weight)
      .toString(16)
      .padStart(2, '0');
  return `#${channel(0)}${channel(1)}${channel(2)}`;
}

export function gridStep(size: string, fallback: number): number {
  const parsed = Number.parseFloat(size);
  if (!Number.isFinite(parsed) || parsed < 8) return fallback;
  return Math.min(parsed, 160);
}

export function seededRandom(seed: number) {
  let state = seed >>> 0;
  return () => {
    state += 0x6d2b79f5;
    let value = state;
    value = Math.imul(value ^ (value >>> 15), value | 1);
    value ^= value + Math.imul(value ^ (value >>> 7), value | 61);
    return ((value ^ (value >>> 14)) >>> 0) / 4294967296;
  };
}

export function hashString(input: string): number {
  let hash = 2166136261;
  for (let i = 0; i < input.length; i += 1) {
    hash ^= input.charCodeAt(i);
    hash = Math.imul(hash, 16777619);
  }
  return hash >>> 0;
}
