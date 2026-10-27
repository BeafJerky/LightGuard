export type IconLayerVariant = 'grid' | 'dock';

export interface IconCell {
  x: number;
  y: number;
  width: number;
  height: number;
}

const METRICS: Record<IconLayerVariant, { width: number; height: number; gapX: number; gapY: number; rows: number }> = {
  grid: { width: 64, height: 58, gapX: 10, gapY: 6, rows: 3 },
  dock: { width: 36, height: 36, gapX: 6, gapY: 0, rows: 1 },
};

export function iconCells(count: number, variant: IconLayerVariant): IconCell[] {
  const metric = METRICS[variant];
  return Array.from({ length: count }, (_, index) => {
    const col = Math.floor(index / metric.rows);
    const row = index % metric.rows;
    return {
      x: col * (metric.width + metric.gapX),
      y: row * (metric.height + metric.gapY),
      width: metric.width,
      height: metric.height,
    };
  });
}

export function iconLayerSize(count: number, variant: IconLayerVariant) {
  const metric = METRICS[variant];
  const rows = Math.min(count, metric.rows) || 1;
  const cols = Math.max(1, Math.ceil(count / metric.rows));
  return {
    width: cols * metric.width + (cols - 1) * metric.gapX,
    height: rows * metric.height + (rows - 1) * metric.gapY,
  };
}
