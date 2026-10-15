// @vitest-environment happy-dom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactNode } from "react";
import { createMockService } from "../../ipc/mock";
import { SessionProvider } from "../../state/react";
import { createSession } from "../../state/session";
import { useSystemScreen } from "./useSystemScreen";

let mock: ReturnType<typeof createMockService>;
let session: ReturnType<typeof createSession>;
let pending: Promise<unknown>[];
const wrapper = ({ children }: { children: ReactNode }) => <SessionProvider value={session}>{children}</SessionProvider>;
async function perform(work: () => unknown) {
  await act(async () => { await work(); while (pending.length) await Promise.all(pending.splice(0)); });
}
beforeEach(async () => {
  mock = createMockService({ tickMs: 0 }); session = createSession(mock); await session.start();
  pending = []; vi.spyOn(session, "run").mockImplementation((work) => { pending.push(work().catch(() => {})); });
  mock.calls.length = 0;
});
afterEach(async () => { cleanup(); await session.monitor(false); session.dispose(); mock.dispose(); vi.restoreAllMocks(); });

describe("useSystemScreen", () => {
  it("waits for real core resources and projects output capability", async () => {
    const { result } = renderHook(useSystemScreen, { wrapper });
    expect(result.current.view?.config).toBe(session.stores.config.getSnapshot());
    await perform(() => session.configure({ method: "native", output_device: "device-output" }));
    await perform(() => session.stores.methods.set({ ...session.stores.methods.getSnapshot()!, native_devices: [{ id: "device-output", name: "Interface", input_channels: 2, output_channels: 12 }] }));
    expect(result.current.view?.outputChannels).toBe(12);
    await perform(() => session.stores.dsp.set(null));
    expect(result.current.view).toBeNull();
  });

  it("wires every configuration, DSP, layout, undo and navigation action", async () => {
    const { result } = renderHook(useSystemScreen, { wrapper });
    const a = result.current.actions;
    const token = await act(() => a.applyPreset("2.1"));
    expect(token).toEqual(expect.any(String));
    expect(mock.calls).toContainEqual({ method: "config_apply_preset", params: { preset: "2.1", confirm: true } });
    await perform(() => a.undo(token!));
    expect(mock.calls).toContainEqual({ method: "undo", params: { token } });
    await perform(() => a.setSubCount(2));
    expect(mock.calls).toContainEqual({ method: "config_set_sub_count", params: { count: 2, confirm: true } });
    const empty = { index: 0, role: null, size: null, band_index: null, out: null, pol: null };
    for (const [run, patch] of [
      [() => a.setRole(0, "L"), { role: "L" }],
      [() => a.setSize(0, "large"), { size: "large" }],
      [() => a.setOutput(0, 0, 4), { band_index: 0, out: 4 }],
      [() => a.setPolarity(0, 0, "-"), { band_index: 0, pol: "-" }],
    ] as const) {
      await perform(run);
      expect(mock.calls).toContainEqual({ method: "config_set_speaker", params: { ...empty, ...patch } });
    }
    await perform(a.addSpeaker); await perform(() => a.removeSpeaker(0)); await perform(a.renumberOutputs);
    expect(mock.calls).toContainEqual({ method: "config_add_speaker" });
    expect(mock.calls).toContainEqual({ method: "config_remove_speaker", params: { index: 0 } });
    expect(mock.calls).toContainEqual({ method: "config_renumber" });
    await perform(() => a.setDspPreset("custom"));
    expect(mock.calls).toContainEqual({ method: "dsp_set", params: { preset: "custom", fields: null } });
    const fields = session.stores.dsp.getSnapshot()!.fields;
    await perform(() => a.setDspFields(fields));
    expect(mock.calls).toContainEqual({ method: "dsp_set", params: { preset: null, fields } });
    await perform(() => a.setArrangement(13));
    await perform(() => a.setStartDelay("10 초"));
    const timing = session.stores.layout.getSnapshot()!.timing;
    await perform(() => a.setTiming(timing));
    expect(mock.calls).toContainEqual({ method: "layout_set", params: { count: 13, start_delay: null, timing: null, confirm: true } });
    expect(mock.calls).toContainEqual({ method: "layout_set", params: { count: null, start_delay: "10 초", timing: null, confirm: false } });
    expect(mock.calls).toContainEqual({ method: "layout_set", params: { count: null, start_delay: null, timing, confirm: false } });
    await perform(a.next); expect(session.host.getSnapshot().current).toBe("connect");
    // The button says 연결·점검으로 on the mdat path too; that screen leads on to 측정·가져오기.
    await perform(() => { session.host.set({ path: "mdat" }); session.navigate("system"); a.next(); });
    expect(session.host.getSnapshot().current).toBe("connect");
  });

  it("does not write a cancelled patch sheet; reports and rejects destructive failures", async () => {
    const { result } = renderHook(useSystemScreen, { wrapper });
    mock.queueDialog("dialog_save_file", null);
    await perform(result.current.actions.savePatchSheet);
    expect(mock.calls).toContainEqual({ method: "dialog_save_file", params: { kind: "patch_sheet", suggested: "secs_patch_sheet.csv" } });
    expect(mock.calls.some((call) => call.method === "patch_sheet_csv")).toBe(false);
    mock.queueDialog("dialog_save_file", "/patch.csv");
    await perform(result.current.actions.savePatchSheet);
    expect(mock.calls).toContainEqual({ method: "patch_sheet_csv", params: { path: "/patch.csv" } });
    mock.failNext("config_apply_preset", "invalid_request");
    await act(async () => { await expect(result.current.actions.applyPreset("bad")).rejects.toThrow(); });
    expect(session.errors.getSnapshot()?.error.code).toBe("invalid_request");
    mock.failNext("config_renumber", "invalid_request");
    await perform(result.current.actions.renumberOutputs);
    expect(session.errors.getSnapshot()?.error.code).toBe("invalid_request");
  });
});
