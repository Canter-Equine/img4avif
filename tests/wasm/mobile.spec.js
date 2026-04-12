// @ts-check
const { test, expect } = require('@playwright/test');
const fs = require('fs');
const path = require('path');

/**
 * Path to the compiled imagine-avif WASM binary produced by:
 *
 *   cargo build --target wasm32-unknown-unknown
 *
 * With `crate-type = ["rlib", "cdylib"]` in Cargo.toml, Cargo emits a
 * `libimagine_avif.wasm` (cdylib) alongside the rlib.
 */
const WASM_PATH = path.resolve(
  __dirname,
  '../../target/wasm32-unknown-unknown/debug/libimagine_avif.wasm',
);

test.describe('imagine-avif WASM mobile compatibility', () => {
  /**
   * Verify the compiled WASM binary is structurally valid (WebAssembly.validate)
   * and can be instantiated (WebAssembly.compile) in the current browser engine.
   *
   * This runs once per Playwright project:
   *  - ios-safari    → WebKit (same engine as iOS Safari)
   *  - android-chrome → Chromium/Blink (same engine as Chrome on Android)
   *
   * The test confirms that the wasm32-unknown-unknown binary produced by the
   * Rust toolchain is accepted by both mobile browser engines, catching any
   * WASM feature usage that is not yet supported on older WebKit versions.
   */
  test('WASM binary is valid and can be compiled by the browser engine', async ({
    page,
    browserName,
  }) => {
    // Read the compiled WASM bytes from disk.
    let wasmBytes;
    try {
      wasmBytes = fs.readFileSync(WASM_PATH);
    } catch (err) {
      throw new Error(
        `Could not read WASM binary at ${WASM_PATH}.\n` +
          'Run `cargo build --target wasm32-unknown-unknown` first.\n' +
          `Original error: ${err}`,
      );
    }

    await page.goto('about:blank');

    // Intercept a synthetic fetch so the WASM bytes never have to cross the
    // CDP serialisation boundary as a raw array (avoids size limits).
    await page.route('**/imagine_avif.wasm', (route) => {
      route.fulfill({
        status: 200,
        contentType: 'application/wasm',
        body: wasmBytes,
      });
    });

    const result = await page.evaluate(async () => {
      try {
        const response = await fetch('/imagine_avif.wasm');
        const buffer = await response.arrayBuffer();
        const bytes = new Uint8Array(buffer);

        // Structural validation — checks the binary format, not behaviour.
        const valid = WebAssembly.validate(bytes);
        if (!valid) {
          return { ok: false, reason: 'WebAssembly.validate() returned false' };
        }

        // Compilation — verifies the browser engine can process all opcodes.
        await WebAssembly.compile(buffer);

        return { ok: true, byteLength: buffer.byteLength };
      } catch (err) {
        return { ok: false, reason: String(err) };
      }
    });

    console.log(
      `[${browserName}] WASM result: valid=${result.ok}`,
      result.ok ? `size=${result.byteLength} bytes` : `reason=${result.reason}`,
    );

    expect(
      result.ok,
      `WASM binary failed in ${browserName}: ${result.reason ?? ''}`,
    ).toBe(true);
  });
});
