// @ts-check
const { defineConfig, devices } = require('@playwright/test');

/**
 * Playwright configuration for img4avif WASM mobile browser tests.
 *
 * Two browser projects are defined:
 *  - ios-safari   : WebKit engine, iPhone 14 viewport + user-agent
 *                   (mirrors Safari on iOS as used by all iOS browsers)
 *  - android-chrome: Chromium/Blink engine, Pixel 5 viewport + user-agent
 *                   (mirrors Chrome on Android)
 *
 * @see https://playwright.dev/docs/api/class-browsertype
 */
module.exports = defineConfig({
  testDir: '.',
  timeout: 60_000,
  retries: 1,
  reporter: [['list'], ['github']],

  projects: [
    {
      name: 'ios-safari',
      use: {
        ...devices['iPhone 14'],
        browserName: 'webkit',
      },
    },
    {
      name: 'android-chrome',
      use: {
        ...devices['Pixel 5'],
        browserName: 'chromium',
      },
    },
  ],
});
