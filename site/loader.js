// Boots a bevy_pf wasm demo: picks the WebGPU bundle when the browser has a
// working adapter, otherwise falls back to the WebGL2 bundle.
export async function boot(app) {
  const msg = document.getElementById('loading-msg');
  const overlay = document.getElementById('loading');

  let backend = 'webgl2';
  if (navigator.gpu) {
    try {
      if (await navigator.gpu.requestAdapter()) backend = 'webgpu';
    } catch (_) { /* fall back */ }
  }
  if (msg) msg.textContent = `Starting (${backend})…`;

  try {
    const mod = await import(`./wasm/${backend}/${app}.js`);
    await mod.default();
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
      msg.textContent =
        'Could not start the demo — your browser may lack WebGPU/WebGL2. ' + e;
    }
  }
}
