import { defineConfig, devices } from '@playwright/test'

export default defineConfig({
  testDir: './tests/e2e', fullyParallel: true, forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0, reporter: [['list'], ['html', { open: 'never' }]],
  workers: 3,
  use: { baseURL: 'http://127.0.0.1:5187/chainflow-intelligence/', trace: 'retain-on-failure', ...(process.env.PLAYWRIGHT_CHANNEL ? { channel: process.env.PLAYWRIGHT_CHANNEL } : {}) },
  projects: [
    { name: 'desktop-chromium', testIgnore: '**/local-health.spec.ts', use: { ...devices['Desktop Chrome'] } },
    { name: 'mobile-chromium', testIgnore: '**/local-health.spec.ts', use: { ...devices['Pixel 7'] } },
    { name: 'local-desktop-chromium', testMatch: '**/local-health.spec.ts', use: { ...devices['Desktop Chrome'], baseURL:'http://127.0.0.1:5190/chainflow-intelligence/' } },
    { name: 'local-mobile-chromium', testMatch: '**/local-health.spec.ts', use: { ...devices['Pixel 7'], baseURL:'http://127.0.0.1:5190/chainflow-intelligence/' } },
  ],
  webServer: [
    { command: 'npm run dev -- --port 5187 --strictPort', url: 'http://127.0.0.1:5187/chainflow-intelligence/', reuseExistingServer: false, env: { VITE_MODE: 'PUBLIC_SNAPSHOT' }, timeout: 60000 },
    { command: 'npm run dev -- --port 5190 --strictPort', url: 'http://127.0.0.1:5190/chainflow-intelligence/', reuseExistingServer: false, env: { VITE_MODE: 'LOCAL_LIVE' }, timeout: 60000 },
  ],
})
