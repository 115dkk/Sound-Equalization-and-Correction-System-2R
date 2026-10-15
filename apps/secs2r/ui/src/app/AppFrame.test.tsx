// @vitest-environment happy-dom
import { fireEvent, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { renderWith } from "../test/render";
import { AppFrame } from "./AppFrame";
import type { FrameModel, StepId } from "./model";
import { neighbourStep } from "./model";

const model = (current: StepId, overrides: Partial<FrameModel["steps"]> = {}): FrameModel => ({
  current,
  steps: {
    system: { status: "done" },
    connect: { status: "skipped" },
    measure: { status: "ready" },
    calibrate: { status: "blocked", reason: "no_measurements" },
    export: { status: "blocked", reason: "no_result" },
    ...overrides,
  },
  project: {
    preset: "7.1.4",
    subCount: 2,
    outputs: 13,
    positionsDone: 3,
    positionsTotal: 9,
    calibration: "stale",
    path: { kind: "rew", state: "ok" },
    fileName: "거실.secs",
    dirty: true,
  },
});

function setup(current: StepId, locale: "ko" | "en" = "ko", overrides: Partial<FrameModel["steps"]> = {}) {
  const handlers = {
    onNavigate: vi.fn(),
    onHelp: vi.fn(),
    onNew: vi.fn(),
    onOpen: vi.fn(),
    onSave: vi.fn(),
    onSaveAs: vi.fn(),
    primary: vi.fn(),
  };
  renderWith(
    <AppFrame
      model={model(current, overrides)}
      onNavigate={handlers.onNavigate}
      primary={{ id: "step.primary", label: "Go", onClick: handlers.primary }}
      projectActions={{ onNew: handlers.onNew, onOpen: handlers.onOpen, onSave: handlers.onSave, onSaveAs: handlers.onSaveAs }}
      onHelp={handlers.onHelp}
      version={{ version: "0.13.0", prerelease: true }}
      tray={<div />}
    >
      <p>content</p>
    </AppFrame>,
    { locale },
  );
  return handlers;
}

const railButton = (id: StepId) => document.querySelector<HTMLButtonElement>(`[data-ui="frame.rail.${id}"]`)!;

describe("app frame", () => {
  it("draws each rail state: done, current, skipped and blocked with its reason", () => {
    setup("measure");
    const rail = screen.getByRole("navigation", { name: "단계" });
    expect(within(rail).getAllByRole("button")).toHaveLength(5);

    expect(railButton("system").className).toContain("rail-step--done");
    expect(railButton("system").textContent).toContain("완료");

    expect(railButton("measure").getAttribute("aria-current")).toBe("step");
    expect(railButton("measure").className).toContain("rail-step--current");

    expect(railButton("connect").disabled).toBe(true);
    expect(railButton("connect").textContent).toContain("건너뜀");

    expect(railButton("calibrate").className).toContain("rail-step--blocked");
    expect(railButton("calibrate").textContent).toContain("측정 필요");
    expect(railButton("export").textContent).toContain("보정 결과 필요");
  });

  it("shows the blocked reason in the header and disables the primary action", () => {
    setup("calibrate");
    const reason = document.querySelector('[data-ui="frame.header.blocked"]');
    expect(reason?.textContent).toBe("측정한 위치가 없습니다. 측정·가져오기에서 측정하거나 mdat 파일을 가져오세요.");
    const primary = screen.getByRole("button", { name: "Go" }) as HTMLButtonElement;
    expect(primary.disabled).toBe(true);
    expect(primary.getAttribute("aria-describedby")).toBe(reason?.id);
  });

  it("keeps the primary action available on a step that is not blocked", () => {
    const handlers = setup("measure");
    const primary = screen.getByRole("button", { name: "Go" }) as HTMLButtonElement;
    expect(primary.disabled).toBe(false);
    fireEvent.click(primary);
    expect(handlers.primary).toHaveBeenCalledOnce();
    expect(document.querySelector('[data-ui="frame.header.blocked"]')).toBeNull();
  });

  it("navigates from the rail and does not open skipped steps", () => {
    const handlers = setup("measure");
    fireEvent.click(railButton("calibrate"));
    expect(handlers.onNavigate).toHaveBeenCalledWith("calibrate");
    fireEvent.click(railButton("connect"));
    expect(handlers.onNavigate).not.toHaveBeenCalledWith("connect");
  });

  it("answers the shortcut registry", () => {
    const handlers = setup("measure");
    fireEvent.keyDown(window, { key: "1", code: "Digit1", ctrlKey: true });
    expect(handlers.onNavigate).toHaveBeenLastCalledWith("system");
    fireEvent.keyDown(window, { key: "ArrowLeft", altKey: true });
    expect(handlers.onNavigate).toHaveBeenLastCalledWith("system");
    fireEvent.keyDown(window, { key: "ArrowRight", altKey: true });
    expect(handlers.onNavigate).toHaveBeenLastCalledWith("calibrate");
    fireEvent.keyDown(window, { key: "2", code: "Digit2", ctrlKey: true });
    expect(handlers.onNavigate).not.toHaveBeenCalledWith("connect");
    fireEvent.keyDown(window, { key: "s", ctrlKey: true });
    expect(handlers.onSave).toHaveBeenCalledOnce();
    fireEvent.keyDown(window, { key: "S", ctrlKey: true, shiftKey: true });
    expect(handlers.onSaveAs).toHaveBeenCalledOnce();
    fireEvent.keyDown(window, { key: "o", ctrlKey: true });
    expect(handlers.onOpen).toHaveBeenCalledOnce();
    fireEvent.keyDown(window, { key: "F1" });
    expect(handlers.onHelp).toHaveBeenCalledOnce();
  });

  it("shows the project status as words", () => {
    setup("measure");
    const status = document.querySelector('[data-ui="frame.status"]')!;
    expect(status.textContent).toContain("7.1.4 · 서브 2 · 출력 13");
    expect(status.textContent).toContain("3/9");
    expect(status.textContent).toContain("다시 계산 필요");
    expect(status.textContent).toContain("REW · 연결됨");
    expect(status.textContent).toContain("거실.secs");
    expect(status.textContent).toContain("저장 안 한 변경");
  });

  it("speaks English when asked", () => {
    setup("calibrate", "en");
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("Correction");
    expect(railButton("calibrate").textContent).toContain("Needs measurements");
  });

  it("skips steps that are not on this path when moving by one", () => {
    const steps = model("system").steps;
    expect(neighbourStep(steps, "system", 1)).toBe("measure");
    expect(neighbourStep(steps, "measure", -1)).toBe("system");
    expect(neighbourStep(steps, "export", 1)).toBeNull();
  });
});
