// jsdom 30 enforces the Web storage spec: localStorage throws SecurityError on
// opaque origins (about:blank). Vitest sets url = http://localhost:3000 so
// dom.window.localStorage works, but the populateGlobal getter for localStorage
// can still end up undefined on the global in some environments.
//
// This setup file runs after the jsdom environment is initialized. It
// re-aliases localStorage from the jsdom window to the global if missing.
const jsdom = (globalThis as Record<string, unknown>).jsdom as
  | { window: Record<string, unknown> }
  | undefined
if (jsdom && typeof globalThis.localStorage === 'undefined') {
  Object.defineProperty(globalThis, 'localStorage', {
    get: () => jsdom.window.localStorage,
    set: () => {},
    configurable: true,
  })
}
