// The app frame: the rail on the left (project menu, five steps, project
// status, settings and help), the step on the right (header with the one
// primary action, the step's content, the job tray at the bottom) and a
// slot for a panel over the content's right side (도움말). The frame draws
// what it is given; screens and the IPC wiring decide the model.
import type { ReactNode } from "react";
import { useT } from "../i18n/react";
import { Icon } from "../ui/Icon";
import { StatusPill } from "../ui/StatusPill";
import { ProjectMenu, SettingsButton, type ProjectActions } from "./menus";
import { STEPS, neighbourStep, type FrameModel, type StepId } from "./model";
import { usePrefs } from "./prefs";
import { ProjectStatus } from "./ProjectStatus";
import { StepHeader, type PrimaryAction } from "./StepHeader";
import { StepRail } from "./StepRail";
import { shortcutText, useShortcuts } from "./shortcuts";
import "./app.css";

export type AppFrameProps = {
  model: FrameModel;
  onNavigate: (step: StepId) => void;
  primary?: PrimaryAction | undefined;
  headerStatus?: ReactNode;
  projectActions: ProjectActions;
  onHelp: () => void;
  version: { version: string; prerelease: boolean } | null;
  tray: ReactNode;
  /**
   * A panel beside the step's content (the help panel): it is placed in the
   * area between the step header and the job tray, which is its positioned
   * parent, so it never covers the primary action or the tray's cancel.
   */
  aside?: ReactNode;
  children: ReactNode;
  /** Listen for the keyboard shortcuts (off for all but one frame in the gallery). */
  shortcuts?: boolean;
};

export function AppFrame({
  model,
  onNavigate,
  primary,
  headerStatus,
  projectActions,
  onHelp,
  version,
  tray,
  aside,
  children,
  shortcuts = true,
}: AppFrameProps) {
  const t = useT();
  const prefs = usePrefs();
  // A skipped step (연결·점검 on the mdat path) still opens when asked for, so
  // the method can be changed; only the neighbour shortcuts step over it.
  const go = (step: StepId | null | undefined) => {
    if (step) onNavigate(step);
  };

  useShortcuts(
    {
      "step.prev": () => go(neighbourStep(model.steps, model.current, -1)),
      "step.next": () => go(neighbourStep(model.steps, model.current, 1)),
      "step.1": () => go(STEPS[0]),
      "step.2": () => go(STEPS[1]),
      "step.3": () => go(STEPS[2]),
      "step.4": () => go(STEPS[3]),
      "step.5": () => go(STEPS[4]),
      "project.save": projectActions.onSave,
      "project.save_as": projectActions.onSaveAs,
      "project.open": projectActions.onOpen,
      help: onHelp,
    },
    shortcuts,
  );

  return (
    <div className="app-frame" data-ui="frame">
      <aside className="app-rail">
        <div className="app-rail__top">
          <div className="app-rail__brand">
            <span className="app-rail__name">SECS 2R</span>
            {version && (
              <span className="app-rail__version tabular">
                {t(version.prerelease ? "app.prerelease" : "app.release", { version: version.version })}
              </span>
            )}
            {prefs.accent === "tutorial" && <StatusPill className="app-rail__mode">{t("frame.tutorial")}</StatusPill>}
          </div>
          <ProjectMenu name={model.project.fileName} dirty={model.project.dirty} actions={projectActions} />
        </div>
        <div className="app-rail__middle">
          <StepRail model={model} onNavigate={go} />
          <section className="app-rail__status" aria-labelledby="frame.status.heading">
            <h2 className="app-rail__heading" id="frame.status.heading">
              {t("frame.status")}
            </h2>
            <ProjectStatus project={model.project} />
          </section>
        </div>
        <div className="app-rail__footer">
          <SettingsButton />
          <button type="button" className="rail-tool" onClick={onHelp} data-ui="frame.help" id="frame.help">
            <Icon name="help" size={18} />
            <span>{t("frame.help")}</span>
            <kbd className="ui-kbd rail-tool__key">{shortcutText("help")}</kbd>
          </button>
        </div>
      </aside>
      <main className="app-main" aria-labelledby="frame.header.title">
        <StepHeader step={model.current} state={model.steps[model.current]} primary={primary} status={headerStatus} />
        <div className="app-body">
          <div className="app-content" id="frame.content" data-ui="frame.content">
            {children}
          </div>
          {aside}
        </div>
        {tray}
      </main>
    </div>
  );
}
