// The updater on screen (ADR 0020), each state read from the mock service
// through its updater setter (`updateStatus`): the rail badge in every
// state it is drawn in, the badge in the frame (default and smallest
// window), the notice after an update with and without skipped versions,
// and the settings popover's updates section in each policy and phase.
import { useState, type ReactNode } from "react";
import { SettingsButton } from "../app/menus";
import { STEP_PRIMARY, type FrameModel } from "../app/model";
import { UpdateScope, useUpdateApi } from "../app/update/scope";
import { UpdateBadge } from "../app/update/UpdateBadge";
import { UpdateNoticeDialog } from "../app/update/UpdateNotice";
import { useT } from "../i18n/react";
import type { UpdateStatus } from "../ipc/bindings/UpdateStatus";
import { createMockService, updateStates } from "../ipc/mock";
import { SessionProvider } from "../state/react";
import { createSession } from "../state/session";
import { initialLedger } from "../state/jobLedger";
import { EmptyState } from "../ui";
import { FrameBox } from "./frameSpecimens";
import type { SpecimenDef } from "./Specimen";

/** A CHANGELOG section as the release carries it (Markdown, Korean like the CHANGELOG). */
const SAMPLE_NOTES = [
  "자동 업데이트를 더한 판입니다. 보정 계산 결과와 내보낸 파일은 1.1.0과 같습니다.",
  "",
  "### 더한 것",
  "",
  "- 새 판이 나오면 뒤에서 받고 앱을 닫을 때 설치합니다. 설정의 '업데이트'에서 '알림만'이나 '확인 안 함'으로 바꿀 수 있습니다.",
  "- 업데이트한 뒤 처음 실행하면 바뀐 점이 한 번 나타납니다. 건너뛴 판이 있으면 릴리스 페이지 링크가 함께 나타납니다.",
  "- `secs2r-cli update check`로 새 판을 확인할 수 있습니다.",
  "",
  "### 고친 것",
  "",
  "- 레벨 점검의 모니터가 오류로 멈추면 **재생 중** 표시가 남던 문제를 고쳤습니다.",
  "  이제 재생 표시가 지워지고 마이크 패널에 멈춘 이유가 나타납니다(#120).",
].join("\n");

const withNotes = (status: UpdateStatus): UpdateStatus => ({ ...status, notice: status.notice && { ...status.notice, notes: SAMPLE_NOTES } });

const checking: UpdateStatus = { ...updateStates.upToDate, phase: { kind: "checking" } };
const downloadingUnknown: UpdateStatus = { ...updateStates.upToDate, phase: { kind: "downloading", version: "1.2.0", received: 0, total: null } };
const installing: UpdateStatus = { ...updateStates.upToDate, phase: { kind: "installing", version: "1.2.0" } };
const off: UpdateStatus = { ...updateStates.upToDate, policy: "off", phase: { kind: "off" } };
const idle: UpdateStatus = { ...updateStates.upToDate, phase: { kind: "idle" }, checked_at: null };

/** The mock service with the updater in `status`, a session over it and the update scope. */
function Live({ status, userAction = false, children }: { status: UpdateStatus; userAction?: boolean; children: ReactNode }) {
  // Made once per specimen and kept for the page's life: the mock has no
  // timers (tickMs 0) and the scope's polling stops when it unmounts.
  const [session] = useState(() => createSession(createMockService({ tickMs: 0, updateStatus: status })));
  return (
    <SessionProvider value={session}>
      <UpdateScope userAction={userAction}>{children}</UpdateScope>
    </SessionProvider>
  );
}

function Badge() {
  const api = useUpdateApi();
  return <div className="gal-rail-sample">{api && <UpdateBadge api={api} />}</div>;
}

function Settings() {
  return (
    <div className="gal-rail-sample gal-rail-sample--popover">
      <SettingsButton initiallyOpen inline />
    </div>
  );
}

const frameModel: FrameModel = {
  current: "measure",
  project: {
    preset: "7.1.4",
    subCount: 2,
    outputs: 13,
    positionsDone: 3,
    positionsTotal: 9,
    calibration: "none",
    path: { kind: "rew", state: "ok" },
    fileName: "거실 7.1.4.secs",
    dirty: false,
  },
  steps: {
    system: { status: "done" },
    connect: { status: "done" },
    measure: { status: "ready" },
    calibrate: { status: "ready" },
    export: { status: "blocked", reason: "no_result" },
  },
};

function Frame() {
  const t = useT();
  return (
    <FrameBox frame={frameModel} jobs={initialLedger} header={{ primary: { id: "measure.primary", label: t(STEP_PRIMARY.measure), onClick: () => {} } }}>
      <EmptyState title={t("frame.placeholder.title")} description={t("frame.placeholder.body")} />
    </FrameBox>
  );
}

const badge = (id: string, title: string, status: UpdateStatus, userAction = false): SpecimenDef => ({
  id: `update.badge.${id}`,
  title: `Update badge · ${title}`,
  width: 300,
  render: () => (
    <Live status={status} userAction={userAction}>
      <Badge />
    </Live>
  ),
});

const settings = (id: string, title: string, status: UpdateStatus): SpecimenDef => ({
  id: `update.settings.${id}`,
  title: `Settings · updates · ${title}`,
  width: 380,
  render: () => (
    <Live status={status}>
      <Settings />
    </Live>
  ),
});

export const updateSpecimens: SpecimenDef[] = [
  badge("available", "알림만: a new version, 받아서 설치", updateStates.available),
  badge("downloading", "downloading, size known (plain bar)", updateStates.downloading),
  badge("downloading-unknown", "downloading, size not known yet", downloadingUnknown),
  badge("ready-on-exit", "downloaded, applied on next start (지금 다시 시작)", updateStates.readyOnExit),
  badge("ready-on-request", "downloaded, installed on request (deb: 지금 설치 asks for the password)", updateStates.readyOnRequest),
  badge("installing", "installing after 지금 다시 시작", installing, true),
  badge("portable", "portable copy: a new version, the release page", updateStates.portable),
  badge("failed", "failed after the user's check", updateStates.failed, true),
  {
    id: "update.frame.ready",
    title: "Frame · update badge above 설정 and 도움말 (downloaded, applied on next start)",
    width: 1280,
    height: 800,
    render: () => (
      <Live status={updateStates.readyOnExit}>
        <Frame />
      </Live>
    ),
  },
  {
    id: "update.frame.min",
    title: "Frame · smallest window (1024 × 700) with the longest badge (deb, 지금 설치)",
    width: 1024,
    height: 700,
    render: () => (
      <Live status={updateStates.readyOnRequest}>
        <Frame />
      </Live>
    ),
  },
  {
    id: "update.notice",
    title: "Notice after an update · the version's CHANGELOG section",
    width: 640,
    render: () => <UpdateNoticeDialog inline notice={withNotes(updateStates.notice).notice!} onDismiss={() => {}} onOpenRelease={() => {}} />,
  },
  {
    id: "update.notice.skipped",
    title: "Notice after an update · versions skipped (link to the release page)",
    width: 640,
    render: () => <UpdateNoticeDialog inline notice={withNotes(updateStates.skippedNotice).notice!} onDismiss={() => {}} onOpenRelease={() => {}} />,
  },
  settings("auto", "자동 설치, up to date", updateStates.upToDate),
  settings("notify", "알림만, a version available (받아서 설치)", updateStates.available),
  settings("off", "확인 안 함", off),
  settings("idle", "not checked yet", idle),
  settings("checking", "checking", checking),
  settings("downloading", "downloading", updateStates.downloading),
  settings("ready", "downloaded, applied on next start", updateStates.readyOnExit),
  settings("portable", "portable copy (replaced by hand, release page)", updateStates.portable),
  settings("failed", "failed (the reason behind 자세히)", updateStates.failed),
];
