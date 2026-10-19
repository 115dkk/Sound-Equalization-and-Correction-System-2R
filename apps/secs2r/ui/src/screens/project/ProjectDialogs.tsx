// The project dialogs: 새 프로젝트, 저장 안 한 변경, 튜토리얼 시작 and
// 프로젝트 열기. Titles name the action and buttons repeat it; Esc and the
// close button cancel (the kit Dialog).
import { useState } from "react";
import { errorText } from "../../app/jobs/jobText";
import type { MessageKey } from "../../i18n";
import { useT } from "../../i18n/react";
import { Button, Dialog, ErrorNotice, Icon, Segmented, StatusPill, Stepper } from "../../ui";
import { PRESET_ROLES, isSubRole } from "../../ui/maps/geometry";
import {
  NEW_PROJECT_PRESETS,
  splitProjectPath,
  type NewProjectPreset,
  type ProjectDialog,
  type ProjectDialogsActions,
  type ProjectDialogsView,
  type UnsavedReason,
} from "./types";
import "./project.css";

/** The demo room `project_new { sample: true }` creates (secs-service Session::create). */
const SAMPLE = { preset: "7.1.4" as NewProjectPreset, subCount: 3 };

const defaultSubs = (preset: NewProjectPreset) => (PRESET_ROLES[preset] ?? []).filter(isSubRole).length;

export function ProjectDialogs({ view, actions, inline }: { view: ProjectDialogsView; actions: ProjectDialogsActions; inline?: boolean }) {
  const dialog = view.dialog;
  if (!dialog) return null;
  switch (dialog.kind) {
    case "new":
      return <NewProjectDialog dialog={dialog} view={view} actions={actions} inline={inline} />;
    case "unsaved":
      return <UnsavedDialog dialog={dialog} view={view} actions={actions} inline={inline} />;
    case "tutorial":
      return <TutorialStartDialog dialog={dialog} actions={actions} inline={inline} />;
    case "open":
      return <OpenProjectDialog dialog={dialog} view={view} actions={actions} inline={inline} />;
  }
}

function Problem({ view }: { view: ProjectDialogsView }) {
  const t = useT();
  if (!view.error) return null;
  const text = errorText(t, view.error);
  return <ErrorNotice id="project.error" cause={text.cause} next={text.next} detail={view.error.detail} code={view.error.code} />;
}

/* ── 새 프로젝트 ────────────────────────────────────────────────────────── */

function NewProjectDialog({
  dialog,
  view,
  actions,
  inline,
}: {
  dialog: Extract<ProjectDialog, { kind: "new" }>;
  view: ProjectDialogsView;
  actions: ProjectDialogsActions;
  inline?: boolean | undefined;
}) {
  const t = useT();
  const [start, setStart] = useState<"empty" | "sample">("empty");
  const [preset, setPreset] = useState<NewProjectPreset>(dialog.initial?.preset ?? "2.0");
  const [subs, setSubs] = useState(dialog.initial?.subCount ?? defaultSubs(dialog.initial?.preset ?? "2.0"));
  const sample = start === "sample";
  return (
    <Dialog
      open
      inline={inline}
      id="project.new"
      help={t("project.new.where")}
      onClose={actions.cancel}
      title={t("project.new.title")}
      footer={
        <>
          <Button variant="quiet" data-ui="project.new.cancel" onClick={actions.cancel}>
            {t("ui.dialog.cancel")}
          </Button>
          <Button
            variant="primary"
            icon="folder"
            data-ui="project.new.create"
            busy={view.busy}
            onClick={() => actions.createProject(sample ? { ...SAMPLE, sample: true } : { preset, subCount: subs, sample: false })}
          >
            {t("project.new.create")}
          </Button>
        </>
      }
    >
      <div className="project-form">
        <div className="project-form__field">
          <Segmented
            id="project.new.start"
            data-autofocus
            label={t("project.new.start")}
            help={t(sample ? "project.new.start.sample.hint" : "project.new.start.empty.hint")}
            value={start}
            options={[
              { value: "empty", label: t("project.new.start.empty") },
              { value: "sample", label: t("project.new.start.sample") },
            ]}
            onChange={setStart}
          />
        </div>
        <div className="project-form__row">
          <Segmented
            id="project.new.preset"
            label={t("project.new.preset")}
            value={sample ? SAMPLE.preset : preset}
            options={NEW_PROJECT_PRESETS.map((value) => ({ value, label: value, disabled: sample }))}
            onChange={(value) => {
              setPreset(value);
              setSubs(defaultSubs(value));
            }}
          />
          <Stepper id="project.new.subs" label={t("project.new.subs")} value={sample ? SAMPLE.subCount : subs} min={0} max={8} disabled={sample} onChange={setSubs} />
        </div>
        {sample && <p className="ui-field__hint">{t("project.new.fixed")}</p>}
        <Problem view={view} />
      </div>
    </Dialog>
  );
}

/* ── 저장 안 한 변경 ────────────────────────────────────────────────────── */

const UNSAVED: Record<UnsavedReason, { title: MessageKey; save: MessageKey; discard: MessageKey }> = {
  close: { title: "project.unsaved.title.close", save: "project.unsaved.save.close", discard: "project.unsaved.discard.close" },
  open: { title: "project.unsaved.title.open", save: "project.unsaved.save.open", discard: "project.unsaved.discard.open" },
  new: { title: "project.unsaved.title.new", save: "project.unsaved.save.new", discard: "project.unsaved.discard.new" },
  tutorial: { title: "project.unsaved.title.tutorial", save: "project.unsaved.save.tutorial", discard: "project.unsaved.discard.tutorial" },
};

function UnsavedDialog({
  dialog,
  view,
  actions,
  inline,
}: {
  dialog: Extract<ProjectDialog, { kind: "unsaved" }>;
  view: ProjectDialogsView;
  actions: ProjectDialogsActions;
  inline?: boolean | undefined;
}) {
  const t = useT();
  const text = UNSAVED[dialog.reason];
  return (
    <Dialog
      open
      inline={inline}
      id="project.unsaved"
      onClose={actions.cancel}
      title={t(text.title)}
      footer={
        <>
          <Button variant="quiet" data-ui="project.unsaved.cancel" onClick={actions.cancel}>
            {t("ui.dialog.cancel")}
          </Button>
          <Button variant="secondary" data-ui="project.unsaved.discard" disabled={view.busy} onClick={actions.discardAndContinue}>
            {t(text.discard)}
          </Button>
          <Button variant="primary" icon="save" data-ui="project.unsaved.save" data-autofocus busy={view.busy} onClick={actions.saveAndContinue}>
            {t(text.save)}
          </Button>
        </>
      }
    >
      <div className="project-form">
        <p>
          {dialog.fileName ? t("project.unsaved.body", { name: dialog.fileName }) : t("project.unsaved.body_untitled")}{" "}
          {t(dialog.reason !== "tutorial" ? "project.unsaved.loss" : dialog.fileName ? "project.unsaved.loss_tutorial" : "project.unsaved.loss_tutorial_untitled")}
        </p>
        {!dialog.fileName && <p className="ui-field__hint">{t("project.unsaved.where")}</p>}
        <Problem view={view} />
      </div>
    </Dialog>
  );
}

/* ── 튜토리얼 시작 ──────────────────────────────────────────────────────── */

const TUTORIAL_POINTS: readonly MessageKey[] = [
  "project.tutorial.point.project",
  "project.tutorial.point.measure",
  "project.tutorial.point.blocked",
  "project.tutorial.point.return",
];

function TutorialStartDialog({ dialog, actions, inline }: { dialog: Extract<ProjectDialog, { kind: "tutorial" }>; actions: ProjectDialogsActions; inline?: boolean | undefined }) {
  const t = useT();
  return (
    <Dialog
      open
      inline={inline}
      id="project.tutorial"
      onClose={actions.cancel}
      title={t("project.tutorial.title")}
      footer={
        <>
          <Button variant="quiet" data-ui="project.tutorial.cancel" onClick={actions.cancel}>
            {t("ui.dialog.cancel")}
          </Button>
          <Button variant="primary" icon="play" data-ui="project.tutorial.start" data-autofocus onClick={actions.startTutorial}>
            {t("project.tutorial.start")}
          </Button>
        </>
      }
    >
      <div className="project-form">
        <p>{t("project.tutorial.lead")}</p>
        <ul className="project-points">
          {TUTORIAL_POINTS.map((key) => (
            <li key={key}>{t(key)}</li>
          ))}
        </ul>
        {dialog.dirty && (
          <p className="project-form__note project-form__note--warning">
            <Icon name="warning" size={16} />
            <span>{t("project.tutorial.dirty")}</span>
          </p>
        )}
      </div>
    </Dialog>
  );
}

/* ── 프로젝트 열기 ──────────────────────────────────────────────────────── */

function OpenProjectDialog({
  dialog,
  view,
  actions,
  inline,
}: {
  dialog: Extract<ProjectDialog, { kind: "open" }>;
  view: ProjectDialogsView;
  actions: ProjectDialogsActions;
  inline?: boolean | undefined;
}) {
  const t = useT();
  // Focus starts on the first choice: the first recent project that can open, or 찾아보기 when there is none.
  const firstRecent = dialog.recent.findIndex((path) => path !== dialog.current);
  return (
    <Dialog
      open
      inline={inline}
      id="project.open"
      onClose={actions.cancel}
      title={t("project.open.title")}
      footer={
        <>
          <Button variant="quiet" data-ui="project.open.cancel" onClick={actions.cancel}>
            {t("ui.dialog.cancel")}
          </Button>
          <Button
            variant="secondary"
            icon="folder"
            data-ui="project.open.browse"
            data-autofocus={(!view.busy && firstRecent < 0) || undefined}
            disabled={view.busy}
            onClick={actions.browseProject}
          >
            {t("project.open.browse")}
          </Button>
        </>
      }
    >
      <div className="project-form">
        <section className="project-recent__section" aria-labelledby="project.open.recent.title">
          <h3 className="project-recent__title" id="project.open.recent.title">
            {t("project.open.recent")}
          </h3>
          {dialog.recent.length === 0 ? (
            <div className="project-recent__empty" data-ui="project.open.recent">
              <p>{t("project.open.recent.empty")}</p>
              <p className="ui-field__hint">{t("project.open.recent.empty.body")}</p>
            </div>
          ) : (
            <ul className="project-recent" data-ui="project.open.recent">
              {dialog.recent.map((path, index) => {
                const { name, folder } = splitProjectPath(path);
                const current = path === dialog.current;
                return (
                  <li key={path}>
                    <button
                      type="button"
                      className="project-recent__item"
                      data-ui={`project.open.recent.${index + 1}`}
                      data-autofocus={(!view.busy && index === firstRecent) || undefined}
                      aria-label={t("project.open.recent.item", { name, folder })}
                      disabled={current || view.busy}
                      onClick={() => actions.openRecent(path)}
                    >
                      <Icon name="file" size={18} className="project-recent__icon" />
                      <span className="project-recent__text">
                        <span className="project-recent__name">{name}</span>
                        <span className="project-recent__folder">{folder}</span>
                      </span>
                      {current && <StatusPill>{t("project.open.current")}</StatusPill>}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </section>
        <Problem view={view} />
      </div>
    </Dialog>
  );
}
