import { invoke } from "@tauri-apps/api/core";
import { beforeEach, expect, it, vi } from "vitest";
import { createTauriHostApi } from "./api";
import { createMockHostApi } from "./mockApi";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
beforeEach(() => vi.clearAllMocks());

it("sends the tray labels as the three top-level command fields", async () => {
  vi.mocked(invoke).mockResolvedValue(undefined);
  const labels = { open: "창 열기", open_app: "SECS 2R 열기", quit: "끝내기" };
  await createTauriHostApi().trayLabels(labels);
  expect(invoke).toHaveBeenCalledExactlyOnceWith("tray_labels", labels);
});

it("passes tray command rejections through unchanged", async () => {
  const error = { code: "internal", params: {}, retryable: false, detail: "tray unavailable" };
  vi.mocked(invoke).mockRejectedValueOnce(error);
  await expect(createTauriHostApi().trayLabels({ open: "Open window", open_app: "Open SECS 2R", quit: "Quit" })).rejects.toBe(error);
});

it("records mock label calls and snapshots their payload, including a failed call", async () => {
  const api = createMockHostApi();
  const labels = { open: "Open window", open_app: "Open SECS 2R", quit: "Quit" };
  const original = { ...labels };
  await api.trayLabels(labels);
  labels.open = "changed";
  const error = { code: "internal" as const, params: {}, retryable: false, detail: "tray unavailable" };
  api.failNext("trayLabels", error);
  await expect(api.trayLabels(original)).rejects.toEqual(error);
  expect(api.calls).toEqual(["trayLabels", "trayLabels"]);
  expect(api.trayLabelCalls).toEqual([original, original]);
});
