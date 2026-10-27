/// <reference types="vitest/config" />
import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { resolve } from 'node:path'

// `vite` serves the demo (index.html, made-up data; `?server` reads the fleet
// example, at FLEET_URL or localhost:3001, through the proxy); `vite build`
// builds the library a host embeds.
export default defineConfig(({ command }) => ({
  plugins: [vue()],
  server: {
    proxy: { '/api': process.env.FLEET_URL ?? 'http://127.0.0.1:3001' },
  },
  build:
    command === 'build'
      ? {
          lib: {
            entry: {
              explorer: resolve(import.meta.dirname, 'src/index.ts'),
              core: resolve(import.meta.dirname, 'src/core/index.ts'),
            },
            formats: ['es'],
          },
          rollupOptions: { external: ['vue'] },
        }
      : undefined,
  test: {
    include: ['src/**/*.test.ts'],
    environment: 'node',
  },
}))
