import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests',
  outputDir: '../logs/tmp/browser/results',
  fullyParallel: false,
  workers: 1,
  timeout: 60_000,
  retries: 0,
  reporter: 'list',
  use: {
    headless: true,
    viewport: { width: 1200, height: 900 },
    ...(process.env.NUMERA_EMOJI_MAPPER_BROWSER_CHANNEL ? {channel: process.env.NUMERA_EMOJI_MAPPER_BROWSER_CHANNEL} : {}),
    launchOptions: process.env.NUMERA_BROWSER_EXECUTABLE ? { executablePath: process.env.NUMERA_BROWSER_EXECUTABLE } : {},
  },
});
