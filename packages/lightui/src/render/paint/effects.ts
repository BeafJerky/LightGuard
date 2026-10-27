import type { CanvasFrame } from '../types';
import { hashString, seededRandom, withAlpha } from './colors';

export type EffectsIntensity = 'low' | 'medium' | 'high';

const COUNT: Record<EffectsIntensity, number> = { low: 18, medium: 40, high: 72 };
const ALPHA: Record<EffectsIntensity, number> = { low: 0.24, medium: 0.34, high: 0.46 };

interface Particle {
  nx: number;
  ny: number;
  phase: number;
  sway: number;
  drift: number;
  size: number;
  accent: boolean;
}

const CACHE = new Map<string, Particle[]>();

function particlesFor(themeId: string, intensity: EffectsIntensity): Particle[] {
  const key = `${themeId}:${intensity}`;
  const cached = CACHE.get(key);
  if (cached) return cached;
  const random = seededRandom(hashString(key));
  const count = COUNT[intensity];
  const list: Particle[] = Array.from({ length: count }, () => ({
    nx: random(),
    ny: random(),
    phase: random() * Math.PI * 2,
    sway: 0.6 + random() * 1.6,
    drift: 0.2 + random() * 0.8,
    size: 1 + random() * 2.4,
    accent: random() > 0.62,
  }));
  CACHE.set(key, list);
  return list;
}

export function createDesktopEffectsDraw(intensity: EffectsIntensity = 'low') {
  return function drawDesktopEffects(ctx: CanvasRenderingContext2D, frame: CanvasFrame) {
    const { theme, width, height, time } = frame;
    if (width < 2 || height < 2) return;

    const t = time / 1000;
    const alpha = ALPHA[intensity];
    const square = theme.pack === 'pixel' || theme.pack === 'nes';

    if (square) {
      ctx.save();
      ctx.fillStyle = withAlpha(theme.mode === 'dark' ? '#000000' : '#111111', 0.06);
      for (let y = 1; y < height; y += 3) ctx.fillRect(0, y, width, 1);
      ctx.restore();
    }

    if (theme.pack === 'tech') {
      ctx.save();
      ctx.strokeStyle = withAlpha(theme.color.primary, 0.05);
      ctx.lineWidth = 1;
      const step = 64;
      const offset = (t * 8) % step;
      ctx.beginPath();
      for (let x = offset; x < width; x += step) {
        ctx.moveTo(Math.round(x) + 0.5, 0);
        ctx.lineTo(Math.round(x) + 0.5, height);
      }
      ctx.stroke();
      ctx.restore();
    }

    const particles = particlesFor(theme.id, intensity);
    const points: Array<[number, number]> = [];
    ctx.save();
    for (const particle of particles) {
      const x =
        (particle.nx * width + Math.sin(t * 0.12 * particle.sway + particle.phase) * 34 + t * particle.drift * 6) %
        width;
      const y =
        (particle.ny * height + Math.cos(t * 0.09 * particle.sway + particle.phase) * 26 + t * particle.drift * 4) %
        height;
      points.push([x, y]);
      const color = particle.accent ? theme.color.accent : theme.color.primary;
      ctx.fillStyle = withAlpha(color, alpha * (square ? 0.9 : 0.7));
      if (square) {
        const size = Math.round(particle.size) + 1;
        ctx.fillRect(Math.round(x), Math.round(y), size, size);
      } else {
        ctx.beginPath();
        ctx.arc(x, y, particle.size, 0, Math.PI * 2);
        ctx.fill();
      }
    }

    if (theme.pack === 'tech') {
      ctx.strokeStyle = withAlpha(theme.color.accent, 0.12);
      ctx.lineWidth = 1;
      for (let i = 0; i < points.length; i += 1) {
        for (let j = i + 1; j < points.length; j += 1) {
          const dx = points[i][0] - points[j][0];
          const dy = points[i][1] - points[j][1];
          if (dx * dx + dy * dy > 12100) continue;
          ctx.beginPath();
          ctx.moveTo(points[i][0], points[i][1]);
          ctx.lineTo(points[j][0], points[j][1]);
          ctx.stroke();
        }
      }
    }
    ctx.restore();
  };
}
