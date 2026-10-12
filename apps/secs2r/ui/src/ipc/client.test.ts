import { describe, expect, it } from "vitest";
import { IpcFailure, makeClient } from "./client";

const info = {
  app_name: "SECS 2R",
  version: "0.0.1",
  prerelease: true,
  upstream: {
    name: "Sound Equalization and Correction System 2",
    version: "Beta 1.2.0",
    commit: "60ac53ad3a3d9304da115e5dda3183a475d2d7dd",
    date: "2026-10-12",
  },
  platform: { os: "linux", arch: "x86_64" },
};

describe("ipc client", () => {
  it("sends the request under `request` and unwraps an ok envelope", async () => {
    const calls: unknown[] = [];
    const request = makeClient(async (command, args) => {
      calls.push([command, args]);
      return { ok: true, data: { method: "bootstrap", data: info } };
    });
    await expect(request({ method: "bootstrap" })).resolves.toEqual(info);
    expect(calls).toEqual([["secs_call", { request: { method: "bootstrap" } }]]);
  });

  it("turns an error envelope into IpcFailure with the code kept", async () => {
    const request = makeClient(async () => ({
      ok: false,
      error: { code: "invalid_request", params: {}, retryable: false, detail: "bad" },
    }));
    const failure = await request({ method: "bootstrap" }).catch((error: unknown) => error);
    expect(failure).toBeInstanceOf(IpcFailure);
    expect((failure as IpcFailure).error.code).toBe("invalid_request");
  });
});
