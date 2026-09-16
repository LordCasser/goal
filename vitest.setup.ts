import { afterEach } from "vitest";

// Node 22's jsdom worker can expose an opaque origin without localStorage.
// Keep browser storage behavior deterministic for components that use it for
// display preferences; this is test-only and does not affect the app runtime.
const memoryStorage = {
  values: new Map<string, string>(),
  getItem(key: string) { return this.values.get(key) ?? null; },
  setItem(key: string, value: string) { this.values.set(key, value); },
  removeItem(key: string) { this.values.delete(key); },
  clear() { this.values.clear(); },
};
Object.defineProperty(globalThis, "localStorage", { configurable: true, writable: true, value: memoryStorage });
afterEach(() => memoryStorage.clear());

// jsdom does not implement matchMedia, while the browser shell reads it for
// reduced-motion behavior and tests spy on the browser API directly.
if (typeof window !== "undefined" && typeof window.matchMedia !== "function") {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    writable: true,
    value: (query: string): MediaQueryList => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    }),
  });
}

// Radix Select keeps the highlighted item visible when a list opens. jsdom
// has no layout engine, so provide the browser method without changing app
// behavior.
if (typeof Element !== "undefined" && typeof Element.prototype.scrollIntoView !== "function") {
  Element.prototype.scrollIntoView = () => {};
}
