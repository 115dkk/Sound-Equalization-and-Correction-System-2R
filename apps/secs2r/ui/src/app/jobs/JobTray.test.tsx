// @vitest-environment happy-dom
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createMockService, type MockService } from "../../ipc/mock";
import { renderWith } from "../../test/render";
import { ConnectedJobTray, JobTray } from "./JobTray";
import { createSession, type Session } from "../../state/session";
import { translate } from "../../i18n";
import type { JobProgress } from "../../ipc/bindings/JobProgress";
import { initialLedger, ledgerReducer } from "../../state/jobLedger";
import { trayState } from "./useJobs";

let mock: MockService;
let session: Session;

async function setup(locale: "ko" | "en" = "ko") {
  mock = createMockService({ tickMs: 0 });
  session = createSession(mock);
  await session.start();
  renderWith(<ConnectedJobTray />, { locale, session });
}

const tray = () => document.querySelector('[data-ui="jobs.tray"]')!;

afterEach(() => { session?.dispose(); mock?.dispose(); });

describe("job tray", () => {
  it("asks for the job snapshots once and then only listens", async () => {
    await setup();
    await waitFor(() => expect(mock.calls.filter((c) => c.method === "jobs")).toHaveLength(1));
    expect(screen.getByText("진행 중인 작업 없음")).toBeTruthy();
  });

  it("draws a job from its pushed events: name, stage, progress, then the result", async () => {
    await setup();
    await act(async () => {
      await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } });
    });
    expect(tray().textContent).toContain("보정 계산");
    act(() => mock.tick());
    expect(tray().textContent).toContain("채널 필터 설계");
    expect(tray().textContent).toContain("1/12");
    const bar = screen.getByRole("progressbar", { name: "보정 계산" });
    expect(bar.getAttribute("aria-valuenow")).toBe("1");
    expect(bar.getAttribute("aria-valuemax")).toBe("12");
    act(() => mock.drain());
    expect(tray().textContent).toContain("진행 중인 작업 없음");
    fireEvent.click(screen.getByRole("button", { name: /기록/ }));
    expect(tray().textContent).toContain("보정 계산 완료");
  });

  it("cancels through job_cancel and reports the job as cancelled", async () => {
    await setup();
    let job = 0;
    await act(async () => {
      job = (await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } })).job;
    });
    act(() => mock.tick());
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "취소" }));
    });
    expect(mock.calls).toContainEqual({ method: "job_cancel", params: { job } });
    expect(tray().textContent).toContain("취소하는 중");
    act(() => mock.tick());
    expect(tray().textContent).toContain("진행 중인 작업 없음");
    fireEvent.click(screen.getByRole("button", { name: /기록/ }));
    expect(tray().textContent).toContain("보정 계산 취소됨");
  });

  it("keeps Log notices and errors in the history with cause, next step and details", async () => {
    await setup();
    mock.failJobs("measure", "rew_unreachable", "connection refused (os error 10061)");
    await act(async () => {
      await mock.request({ method: "measure_position", params: { position: 4 } });
    });
    act(() => mock.drain());
    const toggle = screen.getByRole("button", { name: /기록/ });
    expect(toggle.textContent).toContain("오류 1");
    fireEvent.click(toggle);
    const text = tray().textContent ?? "";
    expect(text).toContain("SW2 신호 대 잡음비 17.2 dB");
    expect(text).toContain("측정 실패: REW에 연결하지 못했습니다.");
    expect(text).toContain("REW를 API 모드로 실행한 뒤 다시 시도하십시오.");
    expect(screen.getByText("자세히")).toBeTruthy();
    expect(text).toContain("connection refused (os error 10061)");
  });

  it("writes the tray in English too", async () => {
    await setup("en");
    await act(async () => {
      await mock.request({ method: "recommend_xo" });
    });
    act(() => mock.tick());
    expect(tray().textContent).toContain("XO recommendation");
    expect(tray().textContent).toContain("Crossover candidates");
    expect(screen.getByRole("button", { name: "Cancel" })).toBeTruthy();
  });
});


describe.each(["ko", "en"] as const)("reported export progress (%s)", (locale) => {
  const t = (key: Parameters<typeof translate>[1], params?: Parameters<typeof translate>[2]) => translate(locale, key, params);

  function renderJob(kind: "export" | "apply", progress: JobProgress | null) {
    let ledger = ledgerReducer(initialLedger, { type: "event", at: 0, event: { event: "started", job: 1, kind } });
    if (progress) ledger = ledgerReducer(ledger, { type: "event", at: 1000, event: { event: "progress", job: 1, ...progress } });
    const cancel = vi.fn();
    renderWith(<JobTray api={{ state: trayState(ledger), now: 65_000, cancel, clearHistory: () => {} }} />, { locale });
    const row = document.querySelector<HTMLElement>(`[data-ui="jobs.job.${kind}"]`)!;
    return { row, cancel };
  }

  describe.each(["export", "apply"] as const)("%s", (kind) => {
    it.each([null, { stage: "export.prepare", done: 0, total: 0 }])("keeps name, spinner, elapsed time and cancel without a measured total: %j", (progress) => {
      const { row, cancel } = renderJob(kind, progress);
      expect(within(row).getByText(t(`jobs.kind.${kind}`))).toBeTruthy();
      expect(row.querySelector(".job-row__spinner")).not.toBeNull();
      expect(within(row).getByText(t("jobs.elapsed", { time: "1:05" }))).toBeTruthy();
      expect(within(row).queryByRole("progressbar")).toBeNull();
      expect(row.querySelector(".job-row__count")).toBeNull();
      expect(row.querySelector(".job-row__progress")?.childElementCount).toBe(0);
      expect(row.querySelector(".job-row__progress")?.textContent).toBe("");
      expect(document.querySelector(".ui-progress--indeterminate")).toBeNull();
      const button = within(row).getByRole("button", { name: t("jobs.cancel") });
      expect(button.getAttribute("data-ui")).toBe(`jobs.cancel.${kind}`);
      expect(button.hasAttribute("disabled")).toBe(false);
      fireEvent.click(button);
      expect(cancel).toHaveBeenCalledExactlyOnceWith(1);
    });

    it.each([
      { stage: "export.prepare", done: 0, total: 117, text: "0/117" },
      { stage: "export.prepare", done: 41, total: 117, text: "41/117" },
      { stage: "export.prepare", done: 117, total: 117, text: "117/117" },
      { stage: "filters.convolution", done: 388, total: 1014, text: "388/1014" },
      { stage: "export.write", done: 0, total: 131_596_288, text: "0.0 / 125.5 MB" },
      { stage: "export.write", done: 52_428_800, total: 131_596_288, text: "50.0 / 125.5 MB" },
      { stage: "export.write", done: 131_596_288, total: 131_596_288, text: "125.5 / 125.5 MB" },
    ] as const)("shows $stage at $done/$total as $text in the bar and the count", ({ stage, done, total, text }) => {
      const { row } = renderJob(kind, { stage, done, total });
      expect(within(row).getByText(t(`jobs.stage.${stage}`))).toBeTruthy();
      const bar = within(row).getByRole("progressbar", { name: t(`jobs.kind.${kind}`) });
      expect(bar.getAttribute("aria-valuenow")).toBe(String(done));
      expect(bar.getAttribute("aria-valuemax")).toBe(String(total));
      expect(bar.getAttribute("aria-valuetext")).toBe(text);
      expect(row.querySelector(".job-row__count")?.textContent).toBe(text);
      expect(within(row).getByText(text)).toBeTruthy();
      expect(document.querySelector(".ui-progress--indeterminate")).toBeNull();
    });
  });
});
