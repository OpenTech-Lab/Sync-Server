import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/e2e",
  timeout: 30_000,
  use: {
    baseURL: process.env.DASHBOARD_BASE_URL || "http://localhost:3000",
    trace: "on-first-retry",
  },
  webServer: {
    command: "npm run dev",
    url: process.env.DASHBOARD_BASE_URL || "http://localhost:3000",
    reuseExistingServer: true,
    timeout: 120_000,
  },
});
