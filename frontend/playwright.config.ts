import { defineConfig, devices } from '@playwright/test'

export default defineConfig({
  testDir: './tests/e2e', fullyParallel: true, forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0, reporter: [['list'], ['html', { open: 'never' }]],
  workers: 3,
  use: { baseURL: 'http://127.0.0.1:5187/chainflow-intelligence/', trace: 'retain-on-failure', ...(process.env.PLAYWRIGHT_CHANNEL ? { channel: process.env.PLAYWRIGHT_CHANNEL } : {}) },
  projects: [
    { name: 'desktop-chromium', use: { ...devices['Desktop Chrome'] } },
    { name: 'mobile-chromium', use: { ...devices['Pixel 7'] } },
  ],
  webServer: { command: 'npm run dev -- --port 5187 --strictPort', url: 'http://127.0.0.1:5187/chainflow-intelligence/', reuseExistingServer: false, env: { VITE_MODE: 'PUBLIC_SNAPSHOT' }, timeout: 60000 },
})
