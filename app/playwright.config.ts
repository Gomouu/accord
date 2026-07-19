import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  timeout: 30_000,
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [['list']],
  expect: {
    toHaveScreenshot: {
      maxDiffPixelRatio: 0.02,
      animations: 'disabled',
      caret: 'hide',
    },
  },
  use: {
    ...devices['Desktop Chrome'],
    baseURL: 'http://localhost:1420',
    viewport: { width: 1280, height: 800 },
    locale: 'fr-FR',
    reducedMotion: 'reduce',
  },
  projects: [{ name: 'chromium' }],
  webServer: {
    command: 'npm run dev',
    url: 'http://localhost:1420/ui-showcase.html',
    reuseExistingServer: true,
    timeout: 60_000,
  },
});
