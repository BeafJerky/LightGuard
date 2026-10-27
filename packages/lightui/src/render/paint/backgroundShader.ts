import type { ThemeTokens } from '../../theme/types';
import { gridStep } from './colors';

const VERTEX_SOURCE = `#version 300 es
in vec2 a_pos;
void main() {
  gl_Position = vec4(a_pos, 0.0, 1.0);
}`;

const FRAGMENT_SOURCE = `#version 300 es
precision highp float;
uniform vec2 u_res;
uniform vec3 u_bg;
uniform vec3 u_primary;
uniform vec3 u_accent;
uniform vec3 u_grid;
uniform float u_gridStep;
uniform float u_gridOn;
uniform float u_scan;
uniform float u_dark;
out vec4 outColor;

void main() {
  vec2 uv = gl_FragCoord.xy / u_res;
  vec2 p = vec2(uv.x * u_res.x / u_res.y, 1.0 - uv.y);
  vec3 color = u_bg;

  float g1 = distance(p, vec2(0.18 * u_res.x / u_res.y, -0.1));
  color += u_primary * 0.10 * smoothstep(0.72, 0.0, g1);

  float g2 = distance(p, vec2(0.92 * u_res.x / u_res.y, 0.04));
  color += u_accent * 0.07 * smoothstep(0.62, 0.0, g2);

  if (u_gridOn > 0.5) {
    vec2 coord = gl_FragCoord.xy / u_gridStep;
    vec2 grid = abs(fract(coord - 0.5) - 0.5) / fwidth(coord);
    float line = 1.0 - min(min(grid.x, grid.y), 1.0);
    color += u_grid * line * 0.85;
  }

  if (u_scan > 0.5) {
    float scan = step(0.66, fract(gl_FragCoord.y / 3.0));
    color *= 1.0 - 0.05 * scan;
  }

  float vig = smoothstep(0.92, 0.28, distance(uv, vec2(0.5, 0.45)));
  color *= mix(mix(0.94, 0.78, u_dark), 1.0, vig);
  outColor = vec4(color, 1.0);
}`;

function compile(gl: WebGL2RenderingContext, type: number, source: string) {
  const shader = gl.createShader(type);
  if (!shader) return null;
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    gl.deleteShader(shader);
    return null;
  }
  return shader;
}

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
    const alpha = parts.length > 3 ? parts[3] : 1;
    if ([r, g, b].every((channel) => Number.isFinite(channel))) {
      return [((r ?? 0) / 255) * alpha, ((g ?? 0) / 255) * alpha, ((b ?? 0) / 255) * alpha];
    }
  }
  return [0, 0, 0];
}

export interface BackgroundShaderHandle {
  resize: (width: number, height: number, dpr: number) => void;
  draw: (theme: ThemeTokens) => void;
  dispose: () => void;
}

export function createBackgroundShader(canvas: HTMLCanvasElement): BackgroundShaderHandle | null {
  let gl: WebGL2RenderingContext | null = null;
  try {
    gl = canvas.getContext('webgl2', { alpha: false, antialias: false, depth: false });
  } catch {
    gl = null;
  }
  if (!gl) return null;

  const vertex = compile(gl, gl.VERTEX_SHADER, VERTEX_SOURCE);
  const fragment = compile(gl, gl.FRAGMENT_SHADER, FRAGMENT_SOURCE);
  if (!vertex || !fragment) {
    if (vertex) gl.deleteShader(vertex);
    if (fragment) gl.deleteShader(fragment);
    return null;
  }

  const program = gl.createProgram();
  if (!program) return null;
  gl.attachShader(program, vertex);
  gl.attachShader(program, fragment);
  gl.linkProgram(program);
  gl.deleteShader(vertex);
  gl.deleteShader(fragment);
  if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
    gl.deleteProgram(program);
    return null;
  }

  const buffer = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
  const position = gl.getAttribLocation(program, 'a_pos');
  gl.enableVertexAttribArray(position);
  gl.vertexAttribPointer(position, 2, gl.FLOAT, false, 0, 0);

  const uniform = (name: string) => gl?.getUniformLocation(program, name) ?? null;
  const locations = {
    res: uniform('u_res'),
    bg: uniform('u_bg'),
    primary: uniform('u_primary'),
    accent: uniform('u_accent'),
    grid: uniform('u_grid'),
    gridStep: uniform('u_gridStep'),
    gridOn: uniform('u_gridOn'),
    scan: uniform('u_scan'),
    dark: uniform('u_dark'),
  };

  gl.useProgram(program);

  return {
    resize(width, height, dpr) {
      if (!gl) return;
      const pixelWidth = Math.max(1, Math.round(width * dpr));
      const pixelHeight = Math.max(1, Math.round(height * dpr));
      if (canvas.width !== pixelWidth) canvas.width = pixelWidth;
      if (canvas.height !== pixelHeight) canvas.height = pixelHeight;
      gl.viewport(0, 0, pixelWidth, pixelHeight);
      gl.uniform2f(locations.res, pixelWidth, pixelHeight);
    },
    draw(theme) {
      if (!gl) return;
      const { color, mode, pack, surface } = theme;
      gl.uniform3fv(locations.bg, toRgb(color.bg));
      gl.uniform3fv(locations.primary, toRgb(color.primary));
      gl.uniform3fv(locations.accent, toRgb(color.accent));
      gl.uniform3fv(locations.grid, toRgb(color.chartGrid));
      gl.uniform1f(locations.gridStep, gridStep(surface.appGridSize, 48));
      gl.uniform1f(locations.gridOn, surface.appGridImage !== 'none' ? 1 : 0);
      gl.uniform1f(locations.scan, pack === 'pixel' || pack === 'nes' ? 1 : 0);
      gl.uniform1f(locations.dark, mode === 'dark' ? 1 : 0);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
    },
    dispose() {
      if (!gl) return;
      gl.deleteBuffer(buffer);
      gl.deleteProgram(program);
      gl = null;
    },
  };
}
