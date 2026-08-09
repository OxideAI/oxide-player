/// <reference types="vitest/config" />
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { VitePWA } from 'vite-plugin-pwa'

export default defineConfig({
  plugins: [
    react(),
    VitePWA({
      registerType: 'autoUpdate',
      includeAssets: ['icon.svg', 'icon-192.png', 'icon-512.png', 'icon-maskable-512.png'],
      manifest: {
        short_name: 'Oxide',
        name: 'Oxide Player',
        description: 'Audiophile music player controlling MPD + CamillaDSP.',
        display: 'standalone',
        background_color: '#050507',
        theme_color: '#050507',
        start_url: '/',
        scope: '/',
        orientation: 'any',
        icons: [
          { src: 'icon-192.png', sizes: '192x192', type: 'image/png' },
          { src: 'icon-512.png', sizes: '512x512', type: 'image/png' },
          { src: 'icon-maskable-512.png', sizes: '512x512', type: 'image/png', purpose: 'maskable' },
          { src: 'icon.svg', sizes: 'any', type: 'image/svg+xml', purpose: 'any' },
        ],
      },
      workbox: {
        // Take control of open pages immediately so a rebuilt bundle (e.g. the
        // Kiosk visualizer) is served without a stale precache gap.
        clientsClaim: true,
        skipWaiting: true,
        globPatterns: ['**/*.{js,css,html,svg,png,woff2}'],
        navigateFallback: '/index.html',
        navigateFallbackDenylist: [/^\/api\//],
        runtimeCaching: [
          // The library snapshot is safe to serve stale: LibraryView paints
          // it immediately, then refreshes from the backend and rewrites the
          // local snapshot when a connection is available.
          {
            urlPattern: ({ url }) => url.pathname === '/api/library',
            handler: 'NetworkFirst',
            options: {
              cacheName: 'oxide-library',
              expiration: { maxEntries: 2, maxAgeSeconds: 60 * 60 * 24 * 30 },
              cacheableResponse: { statuses: [200] },
            },
          },
          // Album keys are stable across library reloads. Stale-while-
          // revalidate keeps artwork instant from disk while allowing a
          // rescan to replace it in the background.
          {
            urlPattern: ({ url }) => url.pathname.startsWith('/api/cover/'),
            handler: 'StaleWhileRevalidate',
            options: {
              cacheName: 'oxide-covers',
              expiration: { maxEntries: 1000, maxAgeSeconds: 60 * 60 * 24 * 365 },
              cacheableResponse: { statuses: [200] },
            },
          },
          // Stateful endpoints (status/queue/playback) are never cached, so
          // the UI never shows stale play/pause state while offline.
          {
            urlPattern: ({ url }) =>
              url.pathname.startsWith('/api/') &&
              !url.pathname.startsWith('/api/cover/') &&
              url.pathname !== '/api/library',
            handler: 'NetworkOnly',
          },
        ],
      },
      devOptions: {
        enabled: false,
      },
    }),
  ],
  server: {
    // Bind all interfaces so devices on the LAN (e.g. a phone) can reach the
    // dev server, not just localhost on this machine.
    host: '0.0.0.0',
    proxy: {
      // `ws: true` lets the /api/ws upgrade pass through to the backend;
      // without it the WebSocket connection fails on the client.
      '/api': { target: 'http://127.0.0.1:8000', ws: true },
    },
  },
  build: {
    outDir: 'dist',
  },
  test: {
    css: true,
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/__tests__/setup.ts'],
  },
})
