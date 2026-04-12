// @ts-check
const { test, expect } = require('@playwright/test');
const fs = require('fs');
const path = require('path');

/**
 * Locate the compiled img4avif WASM binary produced by:
 *
 *   cargo build --target wasm32-unknown-unknown
 *
 * With `crate-type = ["rlib", "cdylib"]` in Cargo.toml, Cargo emits a
 * `.wasm` cdylib into target/wasm32-unknown-unknown/debug/.  The exact
 * filename depends on how Cargo names the cdylib output (e.g. `libimg4avif.wasm`
 * or `img4avif.wasm`), so we search for it rather than hard-coding the name.
 */
const WASM_DIR = path.resolve(__dirname, '../../target/wasm32-unknown-unknown/debug');

function findWasmBinary() {
  if (!fs.existsSync(WASM_DIR)) {
    throw new Error(
      `WASM output directory not found: ${WASM_DIR}\n` +
        'Run `cargo build --target wasm32-unknown-unknown` first.',
    );
  }
  const wasmFiles = fs.readdirSync(WASM_DIR).filter((f) => f.endsWith('.wasm'));
  if (wasmFiles.length === 0) {
    throw new Error(
      `No .wasm file found in ${WASM_DIR}.\n` +
        'Run `cargo build --target wasm32-unknown-unknown` first.',
    );
  }
  // Prefer the cdylib output (usually the only .wasm in the dir).
  return path.join(WASM_DIR, wasmFiles[0]);
}

const WASM_PATH = findWasmBinary();

test.describe('img4avif WASM mobile compatibility', () => {
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
    await page.route('**/img4avif.wasm', (route) => {
      route.fulfill({
        status: 200,
        contentType: 'application/wasm',
        body: wasmBytes,
      });
    });

    const result = await page.evaluate(async () => {
      try {
        const response = await fetch('/img4avif.wasm');
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
