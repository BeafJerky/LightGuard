import type { ThemeTokens } from '../../theme/types';
import { gridStep } from './colors';

const SHADER = `struct Params {
  res: vec2f,
  gridStep: f32,
  gridOn: f32,
  bg: vec4f,
  primary: vec4f,
  accent: vec4f,
  grid: vec4f,
  scan: f32,
  dark: f32,
  pad0: f32,
  pad1: f32,
};

@group(0) @binding(0) var<uniform> p: Params;

@vertex
fn vs(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
  var positions = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
  return vec4f(positions[index], 0.0, 1.0);
}

@fragment
fn fs(@builtin(position) position: vec4f) -> @location(0) vec4f {
  let uv = position.xy / p.res;
  let aspect = p.res.x / p.res.y;
  let point = vec2f(uv.x * aspect, 1.0 - uv.y);
  var color = p.bg.rgb;

  let g1 = distance(point, vec2f(0.18 * aspect, -0.1));
  color += p.primary.rgb * 0.10 * smoothstep(0.72, 0.0, g1);
  let g2 = distance(point, vec2f(0.92 * aspect, 0.04));
  color += p.accent.rgb * 0.07 * smoothstep(0.62, 0.0, g2);

  if (p.gridOn > 0.5) {
    let coord = position.xy / p.gridStep;
    let grid = abs(fract(coord - 0.5) - 0.5) / fwidth(coord);
    let line = 1.0 - min(min(grid.x, grid.y), 1.0);
    color += p.grid.rgb * line * 0.85;
  }

  if (p.scan > 0.5) {
    let scan = step(0.66, fract(position.y / 3.0));
    color *= 1.0 - 0.05 * scan;
  }

  let vig = smoothstep(0.92, 0.28, distance(uv, vec2f(0.5, 0.45)));
  color *= mix(mix(0.94, 0.78, p.dark), 1.0, vig);
  return vec4f(color, 1.0);
}`;

function toRgb(color: string): [number, number, number] {
  const value = color.trim();
  const raw = value.startsWith('#') ? value.slice(1) : '';
  if (raw.length === 6) {
    const int = Number.parseInt(raw, 16);
    if (!Number.isNaN(int)) {
      return [((int >> 16) & 255) / 255, ((int >> 8) & 255) / 255, (int & 255) / 255];
    }
  }
  const rgb = value.match(/rgba?\(([^)]+)\)/);
  if (rgb) {
    const parts = rgb[1].split(',').map((part) => Number.parseFloat(part));
    const [r, g, b] = parts;
    if ([r, g, b].every((channel) => Number.isFinite(channel))) {
      return [(r ?? 0) / 255, (g ?? 0) / 255, (b ?? 0) / 255];
    }
  }
  return [0, 0, 0];
}

interface GpuLike {
  requestAdapter: () => Promise<unknown>;
  getPreferredCanvasFormat: () => string;
}

interface DeviceLike {
  createShaderModule: (input: { code: string }) => unknown;
  createRenderPipeline: (input: unknown) => unknown;
  createBuffer: (input: unknown) => unknown;
  createBindGroup: (input: unknown) => unknown;
  createCommandEncoder: () => unknown;
  queue: { writeBuffer: (buffer: unknown, offset: number, data: Float32Array) => void; submit: (commands: unknown[]) => void };
  destroy?: () => void;
  lost?: Promise<unknown>;
}

interface AdapterLike {
  requestDevice: () => Promise<DeviceLike>;
}

export interface BackgroundShaderWebgpuHandle {
  resize: (width: number, height: number, dpr: number) => void;
  draw: (theme: ThemeTokens) => void;
  dispose: () => void;
}

const BUFFER_FLOATS = 24;

export async function createBackgroundShaderWebgpu(
  canvas: HTMLCanvasElement,
  onLost: () => void,
): Promise<BackgroundShaderWebgpuHandle | null> {
  const gpu = (navigator as unknown as { gpu?: GpuLike }).gpu;
  if (!gpu) return null;
  let device: DeviceLike | null = null;
  try {
    const adapter = (await gpu.requestAdapter()) as AdapterLike | null;
    if (!adapter) return null;
    device = await adapter.requestDevice();
    device.lost?.then(() => onLost());
  } catch {
    return null;
  }

  const context = canvas.getContext('webgpu') as unknown as {
    configure: (input: unknown) => void;
    getCurrentTexture: () => { createView: () => unknown };
  } | null;
  if (!context) {
    device.destroy?.();
    return null;
  }
  const ctx = context;
  let pipeline: { getBindGroupLayout: (index: number) => unknown };
  let buffer: unknown;
  let bindGroup: unknown;
  try {
    const format = gpu.getPreferredCanvasFormat();
    ctx.configure({ device, format, alphaMode: 'opaque' });
    const module = device.createShaderModule({ code: SHADER });
    pipeline = device.createRenderPipeline({
      layout: 'auto',
      vertex: { module, entryPoint: 'vs' },
      fragment: { module, entryPoint: 'fs', targets: [{ format }] },
      primitive: { topology: 'triangle-list' },
    }) as unknown as { getBindGroupLayout: (index: number) => unknown };
    buffer = device.createBuffer({
      size: BUFFER_FLOATS * 4,
      // COPY_DST 必须保留，否则 queue.writeBuffer 校验失败、uniform 全是 0。
      usage: 0x40 | 0x8,
    });
    bindGroup = device.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: [{ binding: 0, resource: { buffer } }],
    });
  } catch {
    device.destroy?.();
    return null;
  }
  const data = new Float32Array(BUFFER_FLOATS);
  let disposed = false;
  let pixelWidth = 1;
  let pixelHeight = 1;

  function resize(nextWidth: number, nextHeight: number, dpr: number) {
    // 片段坐标是物理像素，uniform 必须用画布像素尺寸而不是 CSS 尺寸，
    // 否则 Windows 125% / 150% 缩放下渐变与网格会被放大到画布外。
    pixelWidth = Math.max(1, Math.round(nextWidth * dpr));
    pixelHeight = Math.max(1, Math.round(nextHeight * dpr));
    canvas.width = pixelWidth;
    canvas.height = pixelHeight;
  }

  function draw(theme: ThemeTokens) {
    if (disposed || !device) return;
    const bg = toRgb(theme.color.bg);
    const primary = toRgb(theme.color.primary);
    const accent = toRgb(theme.color.accent);
    const grid = toRgb(theme.color.chartGrid);
    data[0] = pixelWidth;
    data[1] = pixelHeight;
    data[2] = gridStep(theme.surface.appGridSize, 56);
    data[3] = theme.surface.appGridImage !== 'none' ? 1 : 0;
    data.set([...bg, 1], 4);
    data.set([...primary, 1], 8);
    data.set([...accent, 1], 12);
    data.set([...grid, 1], 16);
    data[20] = theme.pack === 'pixel' || theme.pack === 'nes' ? 1 : 0;
    data[21] = theme.mode === 'dark' ? 1 : 0;
    device.queue.writeBuffer(buffer, 0, data);
    const encoder = device.createCommandEncoder() as unknown as {
      beginRenderPass: (input: unknown) => {
        setPipeline: (value: unknown) => void;
        setBindGroup: (index: number, value: unknown) => void;
        draw: (count: number) => void;
        end: () => void;
      };
      finish: () => unknown;
    };
    const pass = encoder.beginRenderPass({
      colorAttachments: [
        {
          view: ctx.getCurrentTexture().createView(),
          loadOp: 'clear',
          storeOp: 'store',
          clearValue: { r: 0, g: 0, b: 0, a: 1 },
        },
      ],
    });
    pass.setPipeline(pipeline);
    pass.setBindGroup(0, bindGroup);
    pass.draw(3);
    pass.end();
    device.queue.submit([encoder.finish()]);
  }

  return {
    resize,
    draw,
    dispose() {
      if (disposed) return;
      disposed = true;
      device?.destroy?.();
      device = null;
    },
  };
}