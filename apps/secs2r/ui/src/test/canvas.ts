// happy-dom has no 2D canvas; uPlot draws on one. `installFakeCanvas()`
// gives every canvas a context that accepts every call (and Path2D likewise),
// so tests keep the real graph, its overlay and its legend. `calls` records
// the context methods named in `record` (e.g. fillRect) for drawing checks.
import { vi } from "vitest";

export type CanvasCall = { method: string; args: unknown[] };

function fakeContext(canvas: HTMLCanvasElement, record: ReadonlySet<string>, calls: CanvasCall[]): CanvasRenderingContext2D {
  const fixed: Record<PropertyKey, unknown> = {
    canvas,
    measureText: () => ({ width: 0, actualBoundingBoxAscent: 0, actualBoundingBoxDescent: 0 }),
    getImageData: () => ({ data: new Uint8ClampedArray(4) }),
    createLinearGradient: () => ({ addColorStop: () => {} }),
    getLineDash: () => [],
  };
  return new Proxy(fixed, {
    get: (target, key) => {
      if (key in target) return target[key];
      if (typeof key === "string" && record.has(key)) return (...args: unknown[]) => void calls.push({ method: key, args });
      return () => {};
    },
    set: (target, key, value) => {
      target[key] = value;
      return true;
    },
  }) as unknown as CanvasRenderingContext2D;
}

/** Path2D likewise (uPlot builds its series paths with it). */
class FakePath2D {
  constructor() {
    return new Proxy(this, { get: (target, key) => (key in target ? (target as Record<PropertyKey, unknown>)[key] : () => {}) });
  }
}

/** Call in `beforeAll`; returns the recorded calls (clear it with `calls.length = 0`). */
export function installFakeCanvas(record: readonly string[] = []): CanvasCall[] {
  const calls: CanvasCall[] = [];
  const names = new Set(record);
  vi.stubGlobal("Path2D", FakePath2D);
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockImplementation(function (this: HTMLCanvasElement) {
    return fakeContext(this, names, calls);
  } as never);
  return calls;
}
