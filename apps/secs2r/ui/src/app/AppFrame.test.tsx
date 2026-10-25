// @vitest-environment happy-dom
import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { renderWith } from "../test/render";
import { AppFrame } from "./AppFrame";
import type { FrameModel, StepId } from "./model";
import { neighbourStep } from "./model";
import { PrefsScope } from "./prefs";
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
  it.each(["ko", "en"] as const)("names measurement file import in project status (%s)", (locale) => {
    renderWith(<ProjectStatus project={{ ...model("system").project, path: { kind: "mdat", state: "ok" } }} />, { locale });
    expect(document.querySelector('[data-ui="frame.status.path"]')?.textContent).toContain(locale === "ko" ? "측정 파일 가져오기" : "Measurement file import");
  });

  it.each(["ko", "en"] as const)("explains that every step needs a project (%s)", (locale) => {
    const blocked = { status: "blocked", reason: "no_project" } as const;
    setup("system", locale, { system: blocked, connect: blocked, measure: blocked, calibrate: blocked, export: blocked });
    for (const step of ["system", "connect", "measure", "calibrate", "export"] as const) expect(railButton(step).textContent).toContain(locale === "ko" ? "프로젝트 열기 필요" : "Needs an open project");
    expect(document.querySelector('[data-ui="frame.header.blocked"]')?.textContent).toContain(locale === "ko" ? "프로젝트 열기 필요" : "Needs an open project");
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
    expect(railButton("export").textContent).toContain("보정 계산 필요");
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

describe("project warnings", () => {
  const outside = (entry: string) => ({ code: "measurement.ir_path.outside", entry });
  const two = [outside("position 3, role L"), outside("position 5, role Tfl")];
  const project = (warnings: readonly { code: string; entry: string }[], opened: number) => ({ ...model("system").project, warnings, opened });
  const ui = (id: string) => document.querySelector<HTMLElement>(`[data-ui="${id}"]`);
  const status = (warnings: readonly { code: string; entry: string }[], opened: number, onRemeasure?: (position: number) => void) => (
    <PrefsScope initial={{ locale: "ko", theme: "light", accent: "default" }}>
      <ProjectStatus project={project(warnings, opened)} onRemeasure={onRemeasure} />
    </PrefsScope>
  );

  it("lists both warnings of a project with their position and role, and opens each position to measure it again", () => {
    const onRemeasure = vi.fn();
    render(status(two, 1, onRemeasure));
    const notice = ui("frame.status.warnings")!;
    expect(notice.getAttribute("role")).toBe("status");
    expect(notice.textContent).toContain("프로젝트 경고 2건");
    expect(notice.textContent).toContain("측정 파일이 프로젝트 폴더 밖에 있어 읽지 않았습니다.");
    expect(ui("frame.status.warnings.position.3")?.textContent).toContain("위치 3");
    expect(ui("frame.status.warnings.position.3")?.textContent).toContain("L");
    // The role as the screen names it (Dolby code), not the service id.
    expect(ui("frame.status.warnings.position.5")?.textContent).toContain("Ltf");
    expect(ui("frame.status.warnings.position.5.fix")?.textContent).toBe("다시 측정");
    fireEvent.click(ui("frame.status.warnings.position.5.fix")!);
    expect(onRemeasure).toHaveBeenCalledWith(5);
  });

  it("gives one line per position, the roles it lost on it, and only the state without an action to run", () => {
    render(status([...two, outside("position 3, role SW1")], 1));
    expect(ui("frame.status.warnings")?.textContent).toContain("프로젝트 경고 3건");
    expect(ui("frame.status.warnings.position.3")?.textContent).toContain("L · SW1");
    expect(document.querySelectorAll('[data-ui^="frame.status.warnings.position."][data-ui$=".fix"]')).toHaveLength(0);
  });

  it("hides the warnings once closed, until the next project is opened", () => {
    const view = render(status(two, 1));
    fireEvent.click(ui("frame.status.warnings.close")!);
    expect(ui("frame.status.warnings")).toBeNull();
    // A reload of the same project keeps them closed.
    view.rerender(status([...two], 1));
    expect(ui("frame.status.warnings")).toBeNull();
    // One raised later in the same project is new and shows on its own.
    view.rerender(status([...two, outside("position 7, role R")], 1));
    expect(ui("frame.status.warnings")?.textContent).toContain("프로젝트 경고 1건");
    expect(ui("frame.status.warnings.position.3")).toBeNull();
    expect(ui("frame.status.warnings.position.7")).not.toBeNull();
    // The next open shows every warning again.
    view.rerender(status(two, 2));
    expect(ui("frame.status.warnings")?.textContent).toContain("프로젝트 경고 2건");
  });

  it("names an unknown code as such and keeps its raw text behind 자세히", () => {
    render(status([{ code: "config.something_new", entry: "role FL, band 0" }], 1));
    const notice = ui("frame.status.warnings")!;
    expect(notice.textContent).toContain("알 수 없는 경고");
    expect(notice.querySelector(".ui-details__raw")?.textContent).toBe("[config.something_new]\nrole FL, band 0");
  });

  it.each(["ko", "en"] as const)("asks to import again on the file import path (%s)", (locale) => {
    renderWith(<ProjectStatus project={{ ...project(two, 1), path: { kind: "mdat", state: "ok" } }} onRemeasure={() => {}} />, { locale });
    expect(ui("frame.status.warnings.position.3.fix")?.textContent).toBe(locale === "ko" ? "다시 가져오기" : "Import again");
  });

  const replaced = (entry: string) => ({ code: "config.band.out.outside", entry });
  const outputs = [replaced("role Lb, band FULL, output 300"), replaced("role SW1, band SUB, output 0")];

  it.each([
    ["ko", "Lrs FULL 대역: 원래 출력 300", "SW1 SUB 대역: 원래 출력 0", "출력 배선 열기"],
    ["en", "Lrs FULL band: was output 300", "SW1 SUB band: was output 0", "Open output patch"],
  ] as const)("lists each replaced output as role, band and the output it had, and opens the output patch (%s)", (locale, first, second, action) => {
    const onOpenPatch = vi.fn();
    renderWith(
      <AppFrame
        model={{ ...model("calibrate"), project: project(outputs, 1) }}
        onNavigate={() => {}}
        projectActions={{ onNew: () => {}, onOpen: () => {}, onSave: () => {}, onSaveAs: () => {} }}
        onHelp={() => {}}
        version={null}
        tray={<div />}
        onOpenPatch={onOpenPatch}
      >
        <p>content</p>
      </AppFrame>,
      { locale },
    );
    expect(ui("frame.status.warnings.band_output.Lrs.FULL")?.textContent).toBe(first);
    expect(ui("frame.status.warnings.band_output.SW1.SUB")?.textContent).toBe(second);
    const buttons = document.querySelectorAll<HTMLButtonElement>('[data-ui="frame.status.warnings.open_patch"]');
    expect(buttons).toHaveLength(1);
    expect(buttons[0]?.textContent).toBe(action);
    fireEvent.click(buttons[0]!);
    expect(onOpenPatch).toHaveBeenCalledTimes(1);
  });

  it("says only what happened to the outputs when there is no patch to open", () => {
    render(status(outputs, 1));
    expect(ui("frame.status.warnings.band_output.Lrs.FULL")?.textContent).toBe("Lrs FULL 대역: 원래 출력 300");
    expect(ui("frame.status.warnings.open_patch")).toBeNull();
  });
});
