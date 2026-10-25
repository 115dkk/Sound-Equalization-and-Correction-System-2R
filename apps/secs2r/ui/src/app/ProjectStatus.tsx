// Project status summary in the rail: setup, measured positions,
// correction state, measurement path and file. Each state is a noun
// phrase; the dot's hue repeats the word, never replaces it. Under it, the
// warnings raised while reading the project (F5): they stay until closed,
// and a closed list comes back with the next project opened.
import { useId, useState, type ReactNode } from "react";
import { useT, type Translate } from "../i18n/react";
import type { MessageKey } from "../i18n";
import { Button, IconButton } from "../ui/Button";
import { Icon, Spinner } from "../ui/Icon";
import type { CalibrationState, PathKind, PathState, ProjectSummary, ProjectWarning } from "./model";
import { groupWarnings, warningKey } from "./projectWarnings";

type Tone = "neutral" | "success" | "warning" | "danger" | "info" | "busy";

const CALIBRATION: Record<CalibrationState, { key: MessageKey; tone: Tone }> = {
  none: { key: "frame.status.calibration.none", tone: "neutral" },
  computing: { key: "frame.status.calibration.computing", tone: "busy" },
  done: { key: "frame.status.calibration.done", tone: "success" },
  stale: { key: "frame.status.calibration.stale", tone: "warning" },
  failed: { key: "frame.status.calibration.failed", tone: "danger" },
};

const PATH_KIND: Record<PathKind, MessageKey> = {
  rew: "frame.status.path.rew",
  native: "frame.status.path.native",
  mock: "frame.status.path.mock",
  mdat: "frame.status.path.mdat",
  none: "frame.status.path.none",
};

const PATH_STATE: Record<PathState, { key: MessageKey; tone: Tone }> = {
  ok: { key: "frame.status.path.ok", tone: "success" },
  warning: { key: "frame.status.path.warning", tone: "warning" },
  error: { key: "frame.status.path.error", tone: "danger" },
};

/** "7.1.4" as is; an edited setup (the service's `custom`) in the word 시스템 uses for it. */
function presetName(t: Translate, preset: string): string {
  return preset === "custom" ? t("system.preset.custom") : preset;
}

function Value({ tone, children }: { tone?: Tone | undefined; children: ReactNode }) {
  return (
    <dd className="status-row__value">
      {tone === "busy" ? (
        <Spinner size={12} className="status-dot status-dot--busy" />
      ) : tone ? (
        <span className={`status-dot status-dot--${tone}`} aria-hidden="true" />
      ) : null}
      <span className="status-row__text">{children}</span>
    </dd>
  );
}

const NOTHING_HIDDEN: ReadonlySet<string> = new Set();

/**
 * The warnings as one notice: a sentence per code, a line per position with
 * the roles it lost and, where measuring it again clears the warning, the
 * action that opens that position; a line per band whose output number was
 * replaced, with one action that opens the output patch. 닫기 hides the
 * warnings shown now; one raised later, or the next project's, shows again.
 */
function ProjectWarnings({
  warnings,
  opened,
  reimport,
  onRemeasure,
  onOpenPatch,
}: {
  warnings: readonly ProjectWarning[];
  opened: number | undefined;
  /** The project's measurements come from files (the action imports again). */
  reimport: boolean;
  onRemeasure?: ((position: number) => void) | undefined;
  onOpenPatch?: (() => void) | undefined;
}) {
  const t = useT();
  const id = useId();
  const [closed, setClosed] = useState<{ opened: number | undefined; keys: ReadonlySet<string> }>({ opened, keys: NOTHING_HIDDEN });
  const hidden = closed.opened === opened ? closed.keys : NOTHING_HIDDEN;
  const shown = warnings.filter((warning) => !hidden.has(warningKey(warning)));
  if (shown.length === 0) return null;
  const close = () => {
    setClosed({ opened, keys: new Set([...hidden, ...shown.map(warningKey)]) });
    // The close button goes with the notice; keep the keyboard in the status.
    const heading = document.getElementById("frame.status.heading");
    if (heading) {
      heading.tabIndex = -1;
      heading.focus({ preventScroll: true });
    }
  };
  return (
    <div className="ui-error ui-error--warning project-warnings" role="status" aria-labelledby={`${id}-title`} data-ui="frame.status.warnings">
      <Icon name="warning" size={18} className="ui-error__icon" />
      <h3 className="ui-error__cause project-warnings__title tabular" id={`${id}-title`}>
        {t("frame.status.warnings.title", { count: shown.length })}
      </h3>
      <IconButton icon="close" size="sm" label={t("frame.status.warnings.close")} className="project-warnings__close" data-ui="frame.status.warnings.close" onClick={close} />
      <div className="project-warnings__body">
        {groupWarnings(t, shown).map((group) => (
          <div key={group.code} className="project-warnings__group">
            <p className="project-warnings__text">{group.text}</p>
            {group.known ? (
              <ul className="project-warnings__list">
                {group.rows.map((row, i) =>
                  "position" in row ? (
                    <li key={row.position} className="project-warnings__row" data-ui={`frame.status.warnings.position.${row.position}`}>
                      <span className="project-warnings__position" id={`${id}-${row.position}`}>
                        {t("frame.status.warnings.position", { n: row.position })}
                      </span>
                      {group.remeasure && onRemeasure && (
                        <Button
                          size="sm"
                          variant="secondary"
                          data-ui={`frame.status.warnings.position.${row.position}.fix`}
                          aria-describedby={`${id}-${row.position} ${id}-${row.position}-roles`}
                          onClick={() => onRemeasure(row.position)}
                        >
                          {t(reimport ? "frame.status.warnings.reimport" : "frame.status.warnings.remeasure")}
                        </Button>
                      )}
                      <span className="project-warnings__roles" id={`${id}-${row.position}-roles`}>
                        {row.roles.join(" · ")}
                      </span>
                    </li>
                  ) : "output" in row ? (
                    <li key={`band.${i}`} className="project-warnings__row" data-ui={`frame.status.warnings.band_output.${row.role}.${row.band}`}>
                      <span className="project-warnings__entry">{t("frame.status.warnings.band_output", { role: row.role, band: row.band, output: row.output })}</span>
                    </li>
                  ) : (
                    <li key={`raw.${i}`} className="project-warnings__row">
                      <span className="project-warnings__entry">{row.raw}</span>
                    </li>
                  ),
                )}
              </ul>
            ) : (
              <details className="ui-details">
                <summary>{t("ui.details")}</summary>
                <pre className="ui-details__raw">
                  {[
                    `[${group.code}]`,
                    ...group.rows.map((row) =>
                      "raw" in row ? row.raw : "output" in row ? `role ${row.role}, band ${row.band}, output ${row.output}` : `position ${row.position}, role ${row.roles.join(", ")}`,
                    ),
                  ].join("\n")}
                </pre>
              </details>
            )}
            {group.known && group.patch && onOpenPatch && (
              <div className="project-warnings__row">
                <Button size="sm" variant="secondary" data-ui="frame.status.warnings.open_patch" onClick={onOpenPatch}>
                  {t("frame.status.warnings.open_patch")}
                </Button>
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

export function ProjectStatus({
  project,
  onRemeasure,
  onOpenPatch,
}: {
  project: ProjectSummary;
  onRemeasure?: ((position: number) => void) | undefined;
  /** The output-number warnings' action: opens 시스템's output patch. */
  onOpenPatch?: (() => void) | undefined;
}) {
  const t = useT();
  const calibration = CALIBRATION[project.calibration];
  const pathState = project.path.kind === "none" ? null : PATH_STATE[project.path.state];
  return (
    <>
      <dl className="project-status" data-ui="frame.status" aria-label={t("frame.status")}>
        <div className="status-row" data-ui="frame.status.config">
          <dt>{t("frame.status.config")}</dt>
          <Value>
            {project.preset
              ? t("frame.status.config.value", { preset: presetName(t, project.preset), subs: project.subCount, outputs: project.outputs })
              : t("frame.status.config.none")}
          </Value>
        </div>
        <div className="status-row" data-ui="frame.status.positions">
          <dt>{t("frame.status.positions")}</dt>
          <Value tone={project.positionsTotal > 0 && project.positionsDone >= project.positionsTotal ? "success" : undefined}>
            {project.positionsTotal === 0 ? (
              t("frame.status.positions.none")
            ) : (
              <span className="tabular">{t("frame.status.positions.value", { done: project.positionsDone, total: project.positionsTotal })}</span>
            )}
          </Value>
        </div>
        <div className="status-row" data-ui="frame.status.calibration">
          <dt>{t("frame.status.calibration")}</dt>
          <Value tone={calibration.tone}>{t(calibration.key)}</Value>
        </div>
        <div className="status-row" data-ui="frame.status.path">
          <dt>{t("frame.status.path")}</dt>
          <Value tone={pathState?.tone ?? "neutral"}>
            {project.path.kind === "none"
              ? t(PATH_KIND.none)
              : t("frame.status.path.value", {
                  kind: project.path.device ? `${t(PATH_KIND[project.path.kind])} · ${project.path.device}` : t(PATH_KIND[project.path.kind]),
                  state: t(pathState?.key ?? "frame.status.path.ok"),
                })}
          </Value>
        </div>
        <div className="status-row" data-ui="frame.status.file">
          <dt>{t("frame.status.file")}</dt>
          <Value tone={project.dirty ? "warning" : undefined}>
            <span className="status-row__file">
              {project.fileName ?? t("frame.status.file.unsaved")}
            </span>
            {project.dirty && project.fileName && <span className="status-row__note">{t("frame.status.file.changed")}</span>}
          </Value>
        </div>
      </dl>
      <ProjectWarnings warnings={project.warnings ?? []} opened={project.opened} reimport={project.path.kind === "mdat"} onRemeasure={onRemeasure} onOpenPatch={onOpenPatch} />
    </>
  );
}
