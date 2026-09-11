import { defineConfig } from 'vitest/config'
import { svelte } from '@sveltejs/vite-plugin-svelte'

// dev 서버에서는 API와 이미지를 로컬 tsuburu 바이너리로 넘긴다.
// 릴리스 빌드에서는 같은 바이너리가 이 dist를 직접 서빙한다.
const backend = 'http://127.0.0.1:8420'

export default defineConfig({
  plugins: [svelte()],
  server: {
    proxy: {
      '/api': backend,
      '/img': backend,
      '/tn': backend,
    },
  },
  build: {
    target: 'es2022',
    // 자산이 바이너리에 박히므로 파일 수를 줄인다.
    assetsInlineLimit: 8192,
  },
  // Components are tested against the client build of svelte, in a DOM, so the
  // effects and bindings under test are the ones that ship.
  resolve: process.env.VITEST ? { conditions: ['browser'] } : {},
  test: {
    environment: 'jsdom',
    setupFiles: ['./src/testing/setup.ts'],
    restoreMocks: true,
  },
})
