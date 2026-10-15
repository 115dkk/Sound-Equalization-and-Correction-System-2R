// Project status summary in the rail: setup, measured positions,
// correction state, measurement path and file. Each state is a noun
// phrase; the dot's hue repeats the word, never replaces it.
import type { ReactNode } from "react";
import { useT, type Translate } from "../i18n/react";
import type { MessageKey } from "../i18n";
import { Spinner } from "../ui/Icon";
import type { CalibrationState, PathKind, PathState, ProjectSummary } from "./model";

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

export function ProjectStatus({ project }: { project: ProjectSummary }) {
  const t = useT();
  const calibration = CALIBRATION[project.calibration];
  const pathState = project.path.kind === "none" ? null : PATH_STATE[project.path.state];
  return (
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
  );
}
