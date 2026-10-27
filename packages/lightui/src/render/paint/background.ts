import type { CanvasFrame } from '../types';
import { gridStep, withAlpha } from './colors';

function fillGlow(
  ctx: CanvasRenderingContext2D,
  frame: CanvasFrame,
  x: number,
  y: number,
  radius: number,
  color: string,
  alpha: number,
) {
  const gradient = ctx.createRadialGradient(x, y, 0, x, y, radius);
  gradient.addColorStop(0, withAlpha(color, alpha));
  gradient.addColorStop(0.62, withAlpha(color, alpha * 0.35));
  gradient.addColorStop(1, withAlpha(color, 0));
  ctx.fillStyle = gradient;
  ctx.fillRect(0, 0, frame.width, frame.height);
}

function drawGrid(ctx: CanvasRenderingContext2D, frame: CanvasFrame) {
  const { theme, width, height } = frame;
  const step = gridStep(theme.surface.appGridSize, 48);
  ctx.save();
  ctx.strokeStyle = theme.color.chartGrid;
  ctx.lineWidth = 1;
  ctx.beginPath();
  for (let x = step; x < width; x += step) {
    ctx.moveTo(Math.round(x) + 0.5, 0);
    ctx.lineTo(Math.round(x) + 0.5, height);
  }
  for (let y = step; y < height; y += step) {
    ctx.moveTo(0, Math.round(y) + 0.5);
    ctx.lineTo(width, Math.round(y) + 0.5);
  }
  ctx.stroke();
  ctx.restore();
}

function drawDither(ctx: CanvasRenderingContext2D, frame: CanvasFrame) {
  const { theme, width, height } = frame;
  ctx.save();
  ctx.fillStyle = withAlpha(theme.color.primary, 0.03);
  const step = 6;
  for (let y = 0; y < height; y += step) {
    for (let x = (y / step) % 2 === 0 ? 0 : step / 2; x < width; x += step) {
      ctx.fillRect(x, y, 1, 1);
    }
  }
  ctx.restore();
}

function drawScanlines(ctx: CanvasRenderingContext2D, frame: CanvasFrame) {
  const { theme, width, height } = frame;
  ctx.save();
  ctx.fillStyle = withAlpha(theme.mode === 'dark' ? '#000000' : '#1a1a1a', 0.05);
  for (let y = 1; y < height; y += 3) ctx.fillRect(0, y, width, 1);
  ctx.restore();
}

export function drawAppBackground(ctx: CanvasRenderingContext2D, frame: CanvasFrame) {
  const { theme, width, height } = frame;
  ctx.fillStyle = theme.color.bg;
  ctx.fillRect(0, 0, width, height);

  fillGlow(ctx, frame, width * 0.18, -height * 0.1, Math.max(width, height) * 0.72, theme.color.primary, 0.1);
  fillGlow(ctx, frame, width * 0.92, height * 0.04, Math.max(width, height) * 0.6, theme.color.accent, 0.07);

  if (theme.surface.appGridImage !== 'none') {
    drawGrid(ctx, frame);
  } else if (theme.pack === 'pixel' || theme.pack === 'nes') {
    drawDither(ctx, frame);
  }

  if (theme.pack === 'pixel' || theme.pack === 'nes') {
    drawScanlines(ctx, frame);
  }

  const vignette = ctx.createRadialGradient(
    width * 0.5,
    height * 0.42,
    Math.min(width, height) * 0.28,
    width * 0.5,
    height * 0.5,
    Math.max(width, height) * 0.78,
  );
  const edge = theme.mode === 'dark' ? 0.22 : 0.05;
  vignette.addColorStop(0, 'rgba(0, 0, 0, 0)');
  vignette.addColorStop(1, `rgba(0, 0, 0, ${edge})`);
  ctx.fillStyle = vignette;
  ctx.fillRect(0, 0, width, height);
}
