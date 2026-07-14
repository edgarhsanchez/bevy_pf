// Boots a bevy_pf wasm demo: picks the WebGPU bundle when the browser has a
// working adapter, otherwise falls back to the WebGL2 bundle. Every deploy
// stamps version.js, so stale cached bundles can never outlive a release.
import { BUILD } from './version.js';

export async function boot(app) {
  const msg = document.getElementById('loading-msg');
  const overlay = document.getElementById('loading');

  // ?backend=webgl2|webgpu forces a bundle (debugging/verification).
  let backend = 'webgl2';
  const forced = new URLSearchParams(location.search).get('backend');
  if (forced === 'webgl2' || forced === 'webgpu') {
    backend = forced;
  } else if (navigator.gpu) {
    try {
      if (await navigator.gpu.requestAdapter()) backend = 'webgpu';
    } catch (_) { /* fall back */ }
  }
  if (msg) msg.textContent = `Starting (${backend})…`;

  try {
    const mod = await import(`./wasm/${backend}/${app}.js?v=${BUILD}`);
    await mod.default({
      module_or_path: new URL(`./wasm/${backend}/${app}_bg.wasm?v=${BUILD}`, import.meta.url),
    });
    overlay?.remove();
  } catch (e) {
    // Bevy/winit escapes main() with a control-flow exception on wasm —
    // that one means the app is up and running.
    if (String(e).includes('Using exceptions for control flow')) {
      overlay?.remove();
      return;
    }
    console.error(e);
    if (msg) {
      const text = String(e);
      msg.textContent = /panic|unreachable/i.test(text)
        ? 'The demo hit a runtime error — details are in the browser console.'
        : 'Could not start the demo — your browser may lack WebGPU/WebGL2. ' + text;
    }
  }
}
