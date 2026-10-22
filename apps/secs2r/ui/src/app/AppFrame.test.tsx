// @vitest-environment happy-dom
import { fireEvent, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { renderWith } from "../test/render";
import { AppFrame } from "./AppFrame";
import type { FrameModel, StepId } from "./model";
import { neighbourStep } from "./model";
import { ProjectStatus } from "./ProjectStatus";

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

function setup(current: StepId, locale: "ko" | "en" = "ko", overrides: Partial<FrameModel["steps"]> = {}, withRecovery = false) {
  const handlers = {
    onNavigate: vi.fn(),
    onHelp: vi.fn(),
    onNew: vi.fn(),
    onOpen: vi.fn(),
    onSave: vi.fn(),
    onSaveAs: vi.fn(),
    primary: vi.fn(),
    recovery: vi.fn(),
  };
  renderWith(
    <AppFrame
      model={model(current, overrides)}
      onNavigate={handlers.onNavigate}
      primary={{ id: "step.primary", label: "Go", onClick: handlers.primary }}
      recovery={withRecovery ? { id: "step.recovery", label: "Measure", onClick: handlers.recovery } : undefined}
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
  it.each(["ko", "en"] as const)("names mdat import in project status (%s)", (locale) => {
    renderWith(<ProjectStatus project={{ ...model("system").project, path: { kind: "mdat", state: "ok" } }} />, { locale });
    expect(document.querySelector('[data-ui="frame.status.path"]')?.textContent).toContain(locale === "ko" ? "mdat 가져오기" : "mdat import");
  });

  it.each(["ko", "en"] as const)("explains that every step needs a project (%s)", (locale) => {
    const blocked = { status: "blocked", reason: "no_project" } as const;
    setup("system", locale, { system: blocked, connect: blocked, measure: blocked, calibrate: blocked, export: blocked });
    for (const step of ["system", "connect", "measure", "calibrate", "export"] as const) expect(railButton(step).textContent).toContain(locale === "ko" ? "프로젝트 필요" : "Needs a project");
    expect(document.querySelector('[data-ui="frame.header.blocked"]')?.textContent).toContain(locale === "ko" ? "프로젝트 필요" : "Needs a project");
  });

  it("draws each rail state: done, current, skipped and blocked with its reason", () => {
    setup("measure");
    const rail = screen.getByRole("navigation", { name: "단계" });
    expect(within(rail).getAllByRole("button")).toHaveLength(5);

    expect(railButton("system").className).toContain("rail-step--done");
    expect(railButton("system").textContent).toContain("완료");

    expect(railButton("measure").getAttribute("aria-current")).toBe("step");
    expect(railButton("measure").className).toContain("rail-step--current");

    expect(railButton("connect").disabled).toBe(false);
    expect(railButton("connect").textContent).toContain("건너뜀");

    expect(railButton("calibrate").className).toContain("rail-step--blocked");
    expect(railButton("calibrate").textContent).toContain("측정 필요");
    expect(railButton("export").textContent).toContain("보정 결과 필요");
  });

  it("shows the blocked reason in the header and disables the primary action", () => {
    setup("calibrate");
    const reason = document.querySelector('[data-ui="frame.header.blocked"]');
    expect(reason?.textContent).toBe("측정 필요");
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

  it("lets the blocked step open its recovery control while the primary stays disabled", () => {
    const handlers = setup("calibrate", "ko", {}, true);
    const recovery = screen.getByRole("button", { name: "Measure" });
    expect((recovery as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(recovery);
    expect(handlers.recovery).toHaveBeenCalledOnce();
    expect((screen.getByRole("button", { name: "Go" }) as HTMLButtonElement).disabled).toBe(true);
    expect(handlers.primary).not.toHaveBeenCalled();
  });

  it("turns the primary off when the screen says so and names the status beside the title as the reason", () => {
    const onClick = vi.fn();
    renderWith(
      <AppFrame
        model={model("measure")}
        onNavigate={() => {}}
        primary={{ id: "step.primary", label: "Go", onClick, disabled: true }}
        headerStatus={<span>MIMO 계산 중</span>}
        projectActions={{ onNew: () => {}, onOpen: () => {}, onSave: () => {}, onSaveAs: () => {} }}
        onHelp={() => {}}
        version={null}
        tray={<div />}
        shortcuts={false}
      >
        <p>content</p>
      </AppFrame>,
    );
    const primary = screen.getByRole("button", { name: "Go" }) as HTMLButtonElement;
    expect(primary.disabled).toBe(true);
    expect(primary.getAttribute("aria-busy")).toBeNull();
    expect(document.getElementById(primary.getAttribute("aria-describedby") ?? "")?.textContent).toBe("MIMO 계산 중");
    fireEvent.click(primary);
    expect(onClick).not.toHaveBeenCalled();
    expect(document.querySelector('[data-ui="frame.header.blocked"]')).toBeNull();
  });

  it("places the aside between the step header and the job tray, beside the content", () => {
    renderWith(
      <AppFrame
        model={model("measure")}
        onNavigate={() => {}}
        projectActions={{ onNew: () => {}, onOpen: () => {}, onSave: () => {}, onSaveAs: () => {} }}
        onHelp={() => {}}
        version={null}
        tray={<div data-ui="jobs.tray" />}
        aside={<aside data-ui="help">help</aside>}
        shortcuts={false}
      >
        <p>content</p>
      </AppFrame>,
    );
    const aside = document.querySelector('[data-ui="help"]')!;
    const content = document.querySelector('[data-ui="frame.content"]')!;
    // Its positioned parent is the box that holds the content: below the header, above the tray.
    expect(aside.parentElement).toBe(content.parentElement);
    expect(content.contains(aside)).toBe(false);
    const body = aside.parentElement!;
    expect(body.className).toBe("app-body");
    expect(body.previousElementSibling?.getAttribute("data-ui")).toBe("frame.header");
    expect(body.nextElementSibling?.getAttribute("data-ui")).toBe("jobs.tray");
  });

  it("navigates from the rail, a skipped step included (to change the method)", () => {
    const handlers = setup("measure");
    fireEvent.click(railButton("calibrate"));
    expect(handlers.onNavigate).toHaveBeenCalledWith("calibrate");
    fireEvent.click(railButton("connect"));
    expect(handlers.onNavigate).toHaveBeenCalledWith("connect");
  });

  it("leaves the keyboard to an open modal dialog", () => {
    const handlers = setup("measure");
    const modal = document.createElement("div");
    modal.setAttribute("role", "dialog");
    modal.setAttribute("aria-modal", "true");
    document.body.appendChild(modal);
    try {
      fireEvent.keyDown(window, { key: "1", code: "Digit1", ctrlKey: true });
      fireEvent.keyDown(window, { key: "s", ctrlKey: true });
      expect(handlers.onNavigate).not.toHaveBeenCalled();
      expect(handlers.onSave).not.toHaveBeenCalled();
    } finally {
      modal.remove();
    }
    fireEvent.keyDown(window, { key: "1", code: "Digit1", ctrlKey: true });
    expect(handlers.onNavigate).toHaveBeenLastCalledWith("system");
  });

  it("answers the shortcut registry", () => {
    const handlers = setup("measure");
    fireEvent.keyDown(window, { key: "1", code: "Digit1", ctrlKey: true });
    expect(handlers.onNavigate).toHaveBeenLastCalledWith("system");
    fireEvent.keyDown(window, { key: "ArrowLeft", altKey: true });
    expect(handlers.onNavigate).toHaveBeenLastCalledWith("system");
    fireEvent.keyDown(window, { key: "ArrowRight", altKey: true });
    expect(handlers.onNavigate).toHaveBeenLastCalledWith("calibrate");
    // Alt+← stepped over the skipped 연결·점검 above; asking for it directly opens it.
    fireEvent.keyDown(window, { key: "2", code: "Digit2", ctrlKey: true });
    expect(handlers.onNavigate).toHaveBeenLastCalledWith("connect");
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
    expect(status.textContent).toContain("미저장 변경 사항");
  });

  it("names an edited setup with the word 시스템 uses, not the raw preset value", () => {
    for (const [locale, text] of [
      ["ko", "사용자 지정 · 서브 2 · 출력 13"],
      ["en", "Custom · Subs: 2 · Outputs: 13"],
    ] as const) {
      const { unmount } = renderWith(<ProjectStatus project={{ ...model("system").project, preset: "custom" }} />, { locale });
      const config = document.querySelector('[data-ui="frame.status.config"]')!;
      expect(config.textContent).toContain(text);
      expect(config.textContent).not.toContain("custom");
      unmount();
    }
  });

  it("speaks English when asked", () => {
    setup("calibrate", "en");
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("Correction");
    expect(railButton("calibrate").textContent).toContain("Not measured");
  });

  it("skips steps that are not on this path when moving by one", () => {
    const steps = model("system").steps;
    expect(neighbourStep(steps, "system", 1)).toBe("measure");
    expect(neighbourStep(steps, "measure", -1)).toBe("system");
    expect(neighbourStep(steps, "export", 1)).toBeNull();
  });
});
