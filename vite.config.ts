import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Tauri ожидает фиксированный порт и не умеет его переопределять на лету,
// поэтому strictPort. Логи vite не чистим, чтобы не затирать вывод cargo.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**'] },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: {
    target: 'es2022',
    // Vite 8 минифицирует через oxc; esbuild больше не идёт в комплекте.
    minify: 'oxc',
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
});
