// @vitest-environment happy-dom
import { cleanup, fireEvent, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { translate } from "../../i18n";
import { renderWith } from "../../test/render";
import { cpuComputeReport } from "../../ipc/fixtures";
import { ComputeControl, ComputeResult } from "./ComputeControl";
import * as fx from "./fixtures";
import { noActions } from "./specimens";

afterEach(cleanup);
describe("compute preference and provenance", () => {
  it("defaults to Automatic with a supported GPU and offers explicit CPU/GPU", () => {
    const changeCompute = vi.fn();
    renderWith(<ComputeControl view={fx.gpuView} actions={{ ...noActions, changeCompute }} />);
    const select = screen.getByRole("combobox") as HTMLSelectElement;
    expect(select.value).toBe("auto");
    expect(select.disabled).toBe(false);
    expect((screen.getByRole("option", { name: "GPU" }) as HTMLOptionElement).disabled).toBe(false);
    expect(screen.getByText(/FIR 가속 장치: GPU/)).toBeTruthy();
    fireEvent.change(select, { target: { value: "cpu" } });
    expect(changeCompute).toHaveBeenCalledWith("cpu");
    fireEvent.change(select, { target: { value: "gpu" } });
    expect(changeCompute).toHaveBeenCalledWith("gpu");
  });
  it("keeps Automatic available and disables unsupported GPU as normal CPU operation", () => {
    renderWith(<ComputeControl view={fx.mimoView} actions={noActions} />);
    expect((screen.getByRole("combobox") as HTMLSelectElement).disabled).toBe(false);
    expect((screen.getByRole("option", { name: "GPU" }) as HTMLOptionElement).disabled).toBe(true);
    expect(screen.getByText("다음 계산: CPU")).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });
  it.each(["ko", "en"] as const)("retains unavailable explicit GPU and offers CPU recovery without claiming automatic fallback (%s)", (locale) => {
    const changeCompute = vi.fn();
    renderWith(<ComputeControl view={fx.requestedGpuUnavailableView} actions={{ ...noActions, changeCompute }} />, { locale });
    const select = screen.getByRole("combobox") as HTMLSelectElement;
    expect(select.value).toBe("gpu");
    expect(select.disabled).toBe(false);
    expect((screen.getByRole("option", { name: "GPU" }) as HTMLOptionElement).disabled).toBe(true);
    expect(screen.getByText(translate(locale, "compute.selected.unavailable"))).toBeTruthy();
    expect(screen.getByText(translate(locale, "compute.selected.unavailable.next"))).toBeTruthy();
    expect(screen.queryByText(translate(locale, "compute.selected.cpu"))).toBeNull();
    expect(screen.queryByText(translate(locale, "compute.reason.runtime_unavailable"))).toBeNull();
    expect(screen.getByText(translate(locale, "compute.install.title"))).toBeTruthy();
    fireEvent.change(select, { target: { value: "auto" } });
    expect(changeCompute).toHaveBeenCalledWith("auto");
    fireEvent.change(select, { target: { value: "cpu" } });
    expect(changeCompute).toHaveBeenCalledWith("cpu");
  });
  it.each(["ko", "en"] as const)("keeps a GPU preference but shows CPU for IIR and the actual IIR result (%s)", (locale) => {
    renderWith(<>
      <ComputeControl view={fx.gpuIirReferenceView} actions={noActions} />
      <ComputeResult report={fx.gpuIirReferenceView.results.mimo!.compute} />
    </>, { locale });
    expect((screen.getByRole("combobox") as HTMLSelectElement).value).toBe("gpu");
    expect((screen.getByRole("option", { name: "GPU" }) as HTMLOptionElement).disabled).toBe(false);
    expect(screen.getByText(translate(locale, "compute.selected.cpu"))).toBeTruthy();
    expect(screen.getByText(translate(locale, "compute.result.cpu"))).toBeTruthy();
    expect(screen.getByText(translate(locale, "compute.reason.iir_reference"))).toBeTruthy();
    expect(screen.getByText(translate(locale, "compute.result.iir_reference"))).toBeTruthy();
    expect(screen.queryByText(translate(locale, "compute.selected.gpu", { device: "NVIDIA GeForce RTX 2080 SUPER" }))).toBeNull();
    expect(screen.queryByText(translate(locale, "compute.result.gpu", { device: "NVIDIA GeForce RTX 2080 SUPER" }))).toBeNull();
  });
  it("shows the available FIR accelerator when FIR is selected", () => {
    const { unmount } = renderWith(<ComputeControl view={fx.gpuIirReferenceView} actions={noActions} />);
    expect(screen.getByText("다음 계산: CPU")).toBeTruthy();
    unmount();
    renderWith(<ComputeControl view={{ ...fx.gpuIirReferenceView, settings: { ...fx.gpuIirReferenceView.settings, mimo_fir: true } }} actions={noActions} />);
    expect((screen.getByRole("combobox") as HTMLSelectElement).value).toBe("gpu");
    expect(screen.getByText(/FIR 가속 장치: GPU/)).toBeTruthy();
  });
  it("shows persisted manual CPU separately from a previous GPU result", () => {
    renderWith(<><ComputeControl view={fx.cpuSelectedView} actions={noActions} /><ComputeResult report={fx.gpuView.results.mimo!.compute} /></>);
    expect((screen.getByRole("combobox") as HTMLSelectElement).value).toBe("cpu");
    expect(screen.getByText("다음 계산: CPU")).toBeTruthy();
    expect(screen.getByText(/지난 결과: GPU FIR 서포트 설계/)).toBeTruthy();
  });
  it.each([fx.runningMimoView, fx.computeLoadingView, { ...fx.mimoView, compute: { ...fx.mimoView.compute, saving: true } }])("disables changes while running, loading or saving", (view) => {
    renderWith(<ComputeControl view={view} actions={noActions} />);
    expect((screen.getByRole("combobox") as HTMLSelectElement).disabled).toBe(true);
  });
  it("shows retryable status errors with raw text only in details", () => {
    const refreshCompute = vi.fn();
    renderWith(<ComputeControl view={fx.computeErrorView} actions={{ ...noActions, refreshCompute }} />);
    expect(screen.getByRole("alert").textContent).toContain("계산 장치를 확인하지 못했습니다.");
    expect(screen.getByText(/device probe failed/).closest("details")?.open).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "다시 확인" }));
    expect(refreshCompute).toHaveBeenCalledOnce();
  });
  it("offers explicit GPU setup help for missing components", () => {
    const openGpuHelp = vi.fn();
    renderWith(<ComputeControl view={fx.runtimeMissingView} actions={{ ...noActions, openGpuHelp }} />, { locale: "en" });
    expect(screen.getByText("Next run: CPU")).toBeTruthy();
    fireEvent.click(screen.getByText("GPU setup"));
    fireEvent.click(screen.getByRole("button", { name: "Open installation guide" }));
    expect(openGpuHelp).toHaveBeenCalledOnce();
    expect(screen.getByText(/CUDA Toolkit 13/)).toBeTruthy();
  });
  it("reports actual CPU fallback despite an available GPU, and explains small CPU jobs", () => {
    const { unmount } = renderWith(<ComputeResult report={fx.gpuFallbackView.results.mimo!.compute} />);
    expect(screen.getByText("지난 결과: CPU 필터 설계")).toBeTruthy();
    expect(screen.getByText(/1개 입력을 CPU로 다시 계산/)).toBeTruthy();
    unmount();
    renderWith(<ComputeResult report={{ ...cpuComputeReport, reason: "small_problem" }} />, { locale: "en" });
    expect(screen.getByText("The calculation was small enough to use the CPU.")).toBeTruthy();
  });
});
