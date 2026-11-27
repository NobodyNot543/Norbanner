import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import * as React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { transformWithEsbuild } from "vite";

// Compile a source module to CommonJS without a WebView, network or Tauri.
const source = fileURLToPath(new URL("../src/", import.meta.url));
async function load(file, stubs = {}) {
  const filename = path.join(source, file);
  const { code } = await transformWithEsbuild(readFileSync(filename, "utf8"), filename, {
    format: "cjs", jsx: "automatic",
  });
  const module = { exports: {} };
  const base = createRequire(filename);
  const require = (id) => (id in stubs ? stubs[id] : base(id));
  new Function("require", "module", "exports", code)(require, module, module.exports);
  return module.exports;
}

const tauri = { "@tauri-apps/api/core": { invoke: async () => null } };

test("every language has exactly the English keys", () => {
  const dir = path.join(source, "i18n");
  const english = JSON.parse(readFileSync(path.join(dir, "en.json"), "utf8"));
  const keys = Object.keys(english).sort();
  for (const file of readdirSync(dir)) {
    const texts = JSON.parse(readFileSync(path.join(dir, file), "utf8"));
    assert.deepEqual(Object.keys(texts).sort(), keys, file);
    assert.equal(texts.variantNames.length, english.variantNames.length, file);
    assert.deepEqual(Object.keys(texts.entryNames).sort(), Object.keys(english.entryNames).sort(), file);
    assert.deepEqual(Object.keys(texts.statusNames).sort(), Object.keys(english.statusNames).sort(), file);
    assert.equal(texts.wallpaperNames.length, 6, file);
  }
});

test("patches merge into nested objects and replace lists", async () => {
  const { merge } = await load("config.ts", tauri);
  const config = { bar: { persistent: true, clock: { showIcon: true, showDate: false }, entries: [{ id: "a", enabled: true }] } };
  const merged = merge(config, { bar: { clock: { showDate: true }, entries: [] } });
  assert.deepEqual(merged, { bar: { persistent: true, clock: { showIcon: true, showDate: true }, entries: [] } });
  // The original is untouched.
  assert.equal(config.bar.clock.showDate, false);
});

test("the list editor renders entries with switches and reorder buttons", async () => {
  const { ListEditor } = await load("controls.tsx");
  const html = renderToStaticMarkup(React.createElement(ListEditor, {
    items: [{ id: "logo", enabled: true }, { id: "clock", enabled: false }],
    names: { logo: "Logo", clock: "Clock" },
    upLabel: "Up", downLabel: "Down", onChange: () => {},
  }));
  assert.match(html, /Logo/);
  assert.match(html, /data-enabled="false"/);
  assert.equal((html.match(/role="switch"/g) ?? []).length, 2);
  // The first item cannot move up, the last cannot move down.
  assert.match(html, /aria-label="Up" disabled/);
  assert.match(html, /aria-label="Down" disabled/);
});

test("the stepper clamps to its range", async () => {
  const { Stepper } = await load("controls.tsx");
  const html = renderToStaticMarkup(React.createElement(Stepper, {
    value: 1, min: 1, max: 10, label: "Shown", onChange: () => {},
  }));
  assert.match(html, /aria-label="−" disabled/);
  assert.match(html, /<output class="stepper-value">1<\/output>/);
});
