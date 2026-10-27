import { fileURLToPath, URL } from 'node:url';
import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';

export default defineConfig({
  plugins: [vue()],
  resolve: {
    alias: {
      '@lightui': fileURLToPath(new URL('../../packages/lightui/src', import.meta.url)),
    },
  },
  server: {
    host: '127.0.0.1',
    fs: {
      allow: ['../..'],
    },
  },
});
