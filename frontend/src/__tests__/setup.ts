// jsdom 30 enforces the Web storage spec: localStorage throws SecurityError on
// opaque origins (about:blank). Vitest sets url = http://localhost:3000 so
// dom.window.localStorage works, but Node 26 also exposes a warning-producing
// global localStorage getter when no --localstorage-file is configured.
//
// This setup file runs after the jsdom environment is initialized. It
// re-aliases localStorage from the jsdom window to avoid that Node getter.
const jsdom = (globalThis as Record<string, unknown>).jsdom as
  | { window: Record<string, unknown> }
  | undefined
if (jsdom) {
  Object.defineProperty(globalThis, 'localStorage', {
    get: () => jsdom.window.localStorage,
    set: () => {},
    configurable: true,
  })
}
