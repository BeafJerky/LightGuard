type Glyph = (ctx: CanvasRenderingContext2D) => void;

function rect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  if (typeof ctx.roundRect === 'function') {
    ctx.roundRect(x, y, w, h, r);
  } else {
    ctx.rect(x, y, w, h);
  }
}

function circle(ctx: CanvasRenderingContext2D, cx: number, cy: number, r: number) {
  ctx.moveTo(cx + r, cy);
  ctx.arc(cx, cy, r, 0, Math.PI * 2);
}

const GLYPHS: Record<string, Glyph> = {
  monitor(ctx) {
    rect(ctx, 3, 4, 18, 12, 2);
    ctx.moveTo(8, 20);
    ctx.lineTo(16, 20);
    ctx.moveTo(12, 16);
    ctx.lineTo(12, 20);
  },
  terminal(ctx) {
    rect(ctx, 3, 4, 18, 16, 2);
    ctx.moveTo(7, 9);
    ctx.lineTo(10, 12);
    ctx.lineTo(7, 15);
    ctx.moveTo(13, 15);
    ctx.lineTo(17, 15);
  },
  folder(ctx) {
    ctx.moveTo(3, 7);
    ctx.quadraticCurveTo(3, 5, 5, 5);
    ctx.lineTo(9.5, 5);
    ctx.lineTo(11.5, 7);
    ctx.lineTo(19, 7);
    ctx.quadraticCurveTo(21, 7, 21, 9);
    ctx.lineTo(21, 17);
    ctx.quadraticCurveTo(21, 19, 19, 19);
    ctx.lineTo(5, 19);
    ctx.quadraticCurveTo(3, 19, 3, 17);
    ctx.closePath();
    ctx.moveTo(3, 10);
    ctx.lineTo(21, 10);
  },
  globe(ctx) {
    circle(ctx, 12, 12, 9);
    ctx.moveTo(3, 12);
    ctx.lineTo(21, 12);
    ctx.moveTo(16.2, 12);
    ctx.ellipse(12, 12, 4.2, 9, 0, 0, Math.PI * 2);
  },
  settings(ctx) {
    ctx.moveTo(4, 7);
    ctx.lineTo(13, 7);
    ctx.moveTo(17, 7);
    ctx.lineTo(20, 7);
    ctx.moveTo(4, 17);
    ctx.lineTo(7, 17);
    ctx.moveTo(11, 17);
    ctx.lineTo(20, 17);
    circle(ctx, 15, 7, 2);
    circle(ctx, 9, 17, 2);
  },
  power(ctx) {
    ctx.moveTo(12, 3);
    ctx.lineTo(12, 12);
    ctx.moveTo(17.2, 6.4);
    ctx.arc(12, 13, 7, -Math.PI * 0.32, Math.PI * 1.32);
  },
  fallback(ctx) {
    rect(ctx, 4, 4, 16, 16, 3);
    ctx.moveTo(8, 12);
    ctx.lineTo(16, 12);
  },
  layers(ctx) {
    ctx.moveTo(12, 3);
    ctx.lineTo(21, 8);
    ctx.lineTo(12, 13);
    ctx.lineTo(3, 8);
    ctx.closePath();
    ctx.moveTo(3, 13);
    ctx.lineTo(12, 18);
    ctx.lineTo(21, 13);
  },
};

export function drawIconGlyph(
  ctx: CanvasRenderingContext2D,
  name: string,
  cx: number,
  cy: number,
  size: number,
) {
  const glyph = GLYPHS[name] ?? GLYPHS.fallback;
  const scale = size / 24;
  ctx.save();
  ctx.translate(cx - size / 2, cy - size / 2);
  ctx.scale(scale, scale);
  ctx.beginPath();
  glyph(ctx);
  ctx.stroke();
  ctx.restore();
}
