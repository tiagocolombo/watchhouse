import {defineConfig} from '@playwright/test';
export default defineConfig({
  testDir:'./tests',testMatch:'**/team.spec.ts',workers:1,
  use:{baseURL:'http://127.0.0.1:8789',browserName:'chromium'},
  webServer:{
    command:'../target/debug/watchhouse --db team-test-results/browser.db serve --port 8789 --manual-workers',
    url:'http://127.0.0.1:8789/',reuseExistingServer:false,
    env:{WATCHHOUSE_TOKEN:'browser-test-operator-token-32-bytes'},
  },
  outputDir:'team-test-results',
});
