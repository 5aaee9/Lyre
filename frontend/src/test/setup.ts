import "@testing-library/jest-dom/vitest";

const localStore = new Map<string, string>();
const sessionStore = new Map<string, string>();

Object.defineProperty(globalThis, "localStorage", {
  value: {
    clear: () => localStore.clear(),
    getItem: (key: string) => localStore.get(key) ?? null,
    removeItem: (key: string) => localStore.delete(key),
    setItem: (key: string, value: string) => localStore.set(key, value)
  },
  configurable: true
});

Object.defineProperty(globalThis, "sessionStorage", {
  value: {
    clear: () => sessionStore.clear(),
    getItem: (key: string) => sessionStore.get(key) ?? null,
    removeItem: (key: string) => sessionStore.delete(key),
    setItem: (key: string, value: string) => sessionStore.set(key, value)
  },
  configurable: true
});

Element.prototype.scrollIntoView = function scrollIntoView() {};
