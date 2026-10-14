// 도움말 over each step (a different tab on each), a search hit, and the
// smallest window.
import type { FrameModel, StepId } from "../../app/model";
import { initialJobs } from "../../app/jobs/jobStore";
import { FrameBox } from "../../gallery/frameSpecimens";
import type { SpecimenDef } from "../../gallery/Specimen";
import { useLocale, useT } from "../../i18n/react";
import { EmptyState } from "../../ui";
import { ExportScreen } from "../export/ExportScreen";
import { exportFrame, exportReady } from "../export/fixtures";
import { noopExportActions } from "../export/specimens";
import { HelpPanel } from "./HelpPanel";
import type { HelpTabId } from "./types";

function helpFrame(step: StepId): FrameModel {
  if (step === "export") return exportFrame(exportReady);
  const after = (s: StepId) => ["system", "connect", "measure", "calibrate", "export"].indexOf(s) > ["system", "connect", "measure", "calibrate", "export"].indexOf(step);
  return {
    current: step,
    project: {
      preset: "7.1.4",
      subCount: 2,
      outputs: 13,
      positionsDone: step === "calibrate" ? 9 : 3,
      positionsTotal: 9,
      calibration: "none",
      path: { kind: "rew", state: "ok" },
      fileName: "거실 7.1.4.secs",
      dirty: true,
    },
    steps: {
      system: { status: "done" },
      connect: after("connect") ? { status: "ready" } : { status: "done" },
      measure: after("measure") ? { status: "ready" } : step === "measure" ? { status: "ready" } : { status: "done" },
      calibrate: step === "calibrate" ? { status: "ready" } : { status: "blocked", reason: "no_measurements" },
      export: { status: "blocked", reason: "no_result" },
    },
  };
}

function Behind({ step }: { step: StepId }) {
  const t = useT();
  if (step === "export") return <ExportScreen view={exportReady} actions={noopExportActions} />;
  return <EmptyState title={t("frame.placeholder.title")} description={t("frame.placeholder.body")} />;
}

function HelpSpecimen({ step, tab, search }: { step: StepId; tab?: HelpTabId; search?: { ko: string; en: string; match?: number } }) {
  const locale = useLocale();
  return (
    <>
      <FrameBox frame={helpFrame(step)} jobs={initialJobs}>
        <Behind step={step} />
      </FrameBox>
      <HelpPanel
        view={{ step, tutorial: false }}
        actions={{ close: () => {}, startTutorial: () => {} }}
        {...(tab ? { initialTab: tab } : {})}
        {...(search ? { initialQuery: search[locale], initialMatch: search.match ?? 0 } : {})}
      />
    </>
  );
}

const size = { width: 1280, height: 800 };

export const helpSpecimens: SpecimenDef[] = [
  { id: "screen.help.system", title: "Help · 시스템, 처음이라면 (tutorial start)", ...size, render: () => <HelpSpecimen step="system" /> },
  { id: "screen.help.connect", title: "Help · 연결·점검, 따라 하기", ...size, render: () => <HelpSpecimen step="connect" tab="steps" /> },
  { id: "screen.help.measure", title: "Help · 측정·가져오기, mdat 가져오기 (channel names)", ...size, render: () => <HelpSpecimen step="measure" tab="mdat" /> },
  { id: "screen.help.calibrate", title: "Help · 보정, 문제 해결", ...size, render: () => <HelpSpecimen step="calibrate" tab="trouble" /> },
  { id: "screen.help.export", title: "Help · 내보내기, CamillaDSP", ...size, render: () => <HelpSpecimen step="export" tab="camilla" /> },
  {
    id: "screen.help.search",
    title: "Help · search hit (second of several, other tabs counted)",
    ...size,
    render: () => <HelpSpecimen step="system" tab="steps" search={{ ko: "극성", en: "polarity", match: 1 }} />,
  },
  { id: "screen.help.export.min", title: "Help · 내보내기, 따라 하기, smallest window (1024 × 700)", width: 1024, height: 700, render: () => <HelpSpecimen step="export" tab="steps" /> },
];
