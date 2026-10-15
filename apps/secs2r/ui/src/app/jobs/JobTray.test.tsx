// @vitest-environment happy-dom
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { createMockService, type MockService } from "../../ipc/mock";
import { renderWith } from "../../test/render";
import { ConnectedJobTray } from "./JobTray";

let mock: MockService;

function setup(locale: "ko" | "en" = "ko") {
  mock = createMockService({ tickMs: 0 });
  renderWith(<ConnectedJobTray />, { locale, ipc: { request: mock.request, events: mock.events } });
}

const tray = () => document.querySelector('[data-ui="jobs.tray"]')!;

afterEach(() => mock?.dispose());

describe("job tray", () => {
  it("asks for the job snapshots once and then only listens", async () => {
    setup();
    await waitFor(() => expect(mock.calls.map((c) => c.method)).toEqual(["jobs"]));
    expect(screen.getByText("진행 중인 작업 없음")).toBeTruthy();
  });

  it("draws a job from its pushed events: name, stage, progress, then the result", async () => {
    setup();
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
    setup();
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
    setup();
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
    expect(text).toContain("REW를 API 모드로 실행한 뒤 다시 시도하세요.");
    expect(screen.getByText("자세히")).toBeTruthy();
    expect(text).toContain("connection refused (os error 10061)");
  });

  it("writes the tray in English too", async () => {
    setup("en");
    await act(async () => {
      await mock.request({ method: "recommend_xo" });
    });
    act(() => mock.tick());
    expect(tray().textContent).toContain("XO recommendation");
    expect(tray().textContent).toContain("Crossover candidates");
    expect(screen.getByRole("button", { name: "Cancel" })).toBeTruthy();
  });
});
