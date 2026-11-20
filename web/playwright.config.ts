import { defineConfig } from '@playwright/test';
export default defineConfig({
  testDir: './tests', testIgnore: '**/team.spec.ts', fullyParallel: false, workers: 1,
  use: { baseURL: 'http://127.0.0.1:8788', browserName: 'chromium' },
  webServer: {
    command: '../target/debug/watchhouse --db test-results/browser.db serve --port 8788',
    url: 'http://127.0.0.1:8788/', reuseExistingServer: false,
    env: { WATCHHOUSE_TOKEN: 'browser-test-operator-token-32-bytes', WATCHHOUSE_SENTRY_FIXTURE_WEBHOOK_SECRET:'sanitized-fixture-secret-32-bytes-only', WATCHHOUSE_SENTRY_FIXTURE_UNSET_TOKEN:'', WATCHHOUSE_DATADOG_FIXTURE_API_KEY:'', WATCHHOUSE_DATADOG_FIXTURE_APP_KEY:'' },
  },
});
