// Test setup: unmount React trees between DOM tests (vitest runs without
// globals, so Testing Library cannot register this itself).
import { afterEach } from "vitest";

if (typeof document !== "undefined") {
  const { cleanup } = await import("@testing-library/react");
  afterEach(() => cleanup());
}
