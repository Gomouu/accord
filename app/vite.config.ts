/// <reference types="vitest/config" />
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Version affichée dans l'app, lue depuis package.json au build : elle reste
// ainsi alignée sur la version publiée sans maintenance manuelle (voir
// src/lib/meta.ts qui consomme la constante injectée `__APP_VERSION__`).
const appVersion = (
  JSON.parse(readFileSync(resolve(process.cwd(), 'package.json'), 'utf-8')) as {
    version: string;
  }
).version;

// Port du serveur de dev fixé pour que `tauri dev` le retrouve.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  define: {
    __APP_VERSION__: JSON.stringify(appVersion),
  },
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: 'es2022',
    outDir: 'dist',
    rollupOptions: {
      output: {
        // Sépare le socle (React) et les dépendances lourdes ponctuelles
        // (`qrcode`, seulement nécessaire pour le QR d'ami, chargé à la
        // demande via React.lazy) du code applicatif : un changement d'UI
        // n'invalide plus le cache du vendor, et `qrcode` ne pèse plus sur le
        // chargement initial.
        manualChunks(id) {
          if (!id.includes('node_modules')) return undefined;
          if (id.includes('qrcode')) return 'qrcode';
          if (id.includes('/react') || id.includes('/scheduler/')) return 'react';
          return 'vendor';
        },
      },
    },
  },
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: ['src/test/setup.ts'],
    css: false,
  },
});
