import { defineConfig } from 'vite';
import basicSsl from '@vitejs/plugin-basic-ssl';
import wasm from 'vite-plugin-wasm';
import path from 'path';

export default defineConfig({
  plugins: [
    wasm(),
    basicSsl()
  ],
  resolve: {
    alias: {
      // During development, resolve @danu/danu to the local build
      // This allows package.json to reference the npm version while using local builds
      '@danu/danu': path.resolve(__dirname, '../package/pkg')
    }
  },
  server: {
    https: true,
    port: 3000,
    fs: {
      allow: [
        // Allow serving files from the frontend directory
        '.',
        // Allow serving the WASM package from the parent directory
        '../package/pkg'
      ]
    }
  },
  build: {
    outDir: 'dist',
    target: 'esnext'
  }
  // Note: This is a client-side only application (no SSR)
});
