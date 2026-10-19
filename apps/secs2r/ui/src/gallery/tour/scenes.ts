// The screen tour's scene table (docs/packets/P43). Each scene mounts the
// real App on the mock service with no automatic tick, brings the mock to a
// state in `setup`, then drives the window the way a user would (`ops`).
// scripts/screen-tour.mjs reads this table from the page and captures every
// scene full-window in four variants; ui-gallery.yml posts the PNGs.
//
// Selectors name stable ids (`data-ui`), never words that change with the
// language. Work advances only where a `page` op ticks or drains the mock,
// so every capture draws the same picture.
import type { StepId } from "../../app/model";
import type { Envelope, Invoke } from "../../ipc/client";
import type { MeasureConfigureParams } from "../../ipc/bindings/MeasureConfigureParams";
import type { MdatScanResult } from "../../ipc/bindings/MdatScanResult";
import type { Request } from "../../ipc/bindings/Request";
import type { Response } from "../../ipc/bindings/Response";
import type { MockService } from "../../ipc/mock";
import { advanceClock } from "./clock";

export type TourOp =
  | { kind: "press"; key: string } // Playwright keyboard.press string, e.g. "Control+3", "Escape", "F1"
  | { kind: "click"; selector: string } // CSS selector; never text that changes with the language
  | { kind: "hover"; selector: string }
  | { kind: "page"; run: (ctx: TourContext) => void | Promise<void> }; // runs in the page (mock ticks and the like)

export type TourContext = { mock: MockService };

export type TourScene = {
  /** Lower case, dots and hyphens; the file name. */
  id: string;
  /** The issue section it goes under. */
  step: StepId | "start" | "frame" | "tutorial";
  /** The scene's name in the issue (a noun phrase). */
  title: { ko: string; en: string };
  /** What to look at, one sentence, only when it is easy to miss. */
  note?: { ko: string; en: string };
  /** The mock's starting project; "sample" (the default) is 7.1.4 + 3 subwoofers, "stereo" a new 2.0 project. */
  project?: "none" | "stereo" | "sample";
  expert?: boolean;
  tutorialStart?: string;
  /** Brings the mock to the scene's state before the App mounts. */
  setup?: (mock: MockService) => void | Promise<void>;
  /** Run in order after the App has mounted. */
  ops?: TourOp[];
};

/** Where the tour's projects live (the save dialog's answer). */
export const PROJECT_PATH = "C:/Users/demo/Documents/SECS/home-theater.secs";

/* ── Helpers ────────────────────────────────────────────────────────────── */

const ui = (id: string) => `[data-ui="${id}"]`;
const press = (key: string): TourOp => ({ kind: "press", key });
const click = (selector: string): TourOp => ({ kind: "click", selector });
const page = (run: (ctx: TourContext) => void | Promise<void>): TourOp => ({ kind: "page", run });

/** One macrotask: every promise the mock resolved (and what it started) has run. */
const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

/** Seconds of work one mock tick stands for on the tour clock (a sweep, an engine stage). */
const TICK_MS = 4000;

/** Advances the mock's jobs one tick at a time, letting the screen answer each. */
const tick = (count: number): TourOp =>
  page(async ({ mock }) => {
    for (let i = 0; i < count; i++) {
      advanceClock(TICK_MS);
      mock.tick();
      await flush();
    }
  });

/** Runs every finite job to its end (live monitors keep running). */
const drain: TourOp = page(async ({ mock }) => {
  advanceClock(5 * TICK_MS);
  mock.drain();
  await flush();
});

/**
 * Scrolls the element's own scroll area (the frame's content) so it sits
 * near the top, as a user scrolling there would. Not scrollIntoView: that
 * also shifts the overflow-hidden frame boxes, which no user can do.
 */
const reveal = (selector: string): TourOp =>
  page(() => {
    const el = document.querySelector<HTMLElement>(selector);
    if (!el) throw new Error(`reveal: nothing matches ${selector}`);
    for (let node = el.parentElement; node; node = node.parentElement) {
      if (!/(auto|scroll)/.test(getComputedStyle(node).overflowY) || node.scrollHeight <= node.clientHeight) continue;
      node.scrollTop = Math.max(0, el.getBoundingClientRect().top - node.getBoundingClientRect().top + node.scrollTop - 16);
      return;
    }
    throw new Error(`reveal: ${selector} has no scroll area`);
  });

const STEP_KEY: Record<StepId, string> = { system: "Control+1", connect: "Control+2", measure: "Control+3", calibrate: "Control+4", export: "Control+5" };
const go = (step: StepId) => press(STEP_KEY[step]);

type Path = "native" | "rew" | "mock" | "mdat";

/** Chooses the measurement method on 연결·점검 the way a user does, then goes to `then`. */
const choosePath = (path: Path, then?: StepId): TourOp[] => [go("connect"), click(ui(`connect.method.${path}`)), ...(then ? [go(then)] : [])];

const MEASURE_DEFAULTS: MeasureConfigureParams = {
  method: "none", input_device: null, output_device: null, input_channel: null, output_channels: [], sweep_length: null,
  repetitions: null, sweep_level_dbfs: null, timing: null, loopback_input_channel: null, timing_ref_output: null, share_mode: null,
};

/** The devices a user picked on 연결·점검 (the mock's UMIK-1 and AVR); the method stays unchosen. */
async function devices(mock: MockService, path: "native" | "rew" = "native") {
  const [input_device, output_device] = path === "rew" ? ["mic", "hdmi"] : ["wasapi:{0.0.1.00000000}.{mic}", "wasapi:{0.0.0.00000000}.{hdmi}"];
  await mock.request({ method: "measure_configure", params: { ...MEASURE_DEFAULTS, input_device, output_device, input_channel: 1 } });
}

/** Every channel has a level (the level check's '기본값으로 채우기'), so 연결·점검 reads done. */
async function levels(mock: MockService) {
  await mock.request({ method: "levels_fill_defaults" });
}

/** The arrangement for one chair (9 positions) instead of the demo room's single position. */
async function chair(mock: MockService) {
  await mock.request({ method: "layout_set", params: { count: 9, start_delay: null, timing: null, confirm: true } });
}

/** The demo room's measurements at positions 1…count. */
async function measured(mock: MockService, count: number | null = null) {
  await mock.request({ method: "sample_load", params: { positions: count } });
  mock.drain();
}

async function calibrated(mock: MockService) {
  await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } });
  mock.drain();
}

/** MIMO calculated on top of the current correction; it goes into the export when switched on, from three positions. */
async function mimo(mock: MockService) {
  await mock.request({ method: "mimo", params: { settings: null } });
  mock.drain();
}

/** A project in progress: devices picked, levels checked, the demo room measured. */
async function inProgress(mock: MockService) {
  await devices(mock);
  await levels(mock);
  await measured(mock);
}

/**
 * Answers one method with a fixed reply instead of the mock's. The tour
 * mounts the App on `mock.invoke` (looked up per call), so a setup can shape
 * a reply the mock does not produce without changing the mock.
 */
function reply<M extends Request["method"]>(mock: MockService, method: M, data: Extract<Response, { method: M }>["data"]) {
  const next: Invoke = mock.invoke;
  mock.invoke = async (command, args) => {
    const request = args.request as Request | undefined;
    if (command === "secs_call" && request?.method === method) return { ok: true, data: { method, data } as Response } satisfies Envelope;
    return next(command, args);
  };
}

/** An REW .mdat with three mains named for position 3 and two subs named for none (they go to the current position). */
const MIXED_MDAT: MdatScanResult = {
  measurements: [
    { index: 0, title: "L P3", role: "L", position: 3, position_tagged: true, looks_like_sub: false },
    { index: 1, title: "R P3", role: "R", position: 3, position_tagged: true, looks_like_sub: false },
    { index: 2, title: "C P3", role: "C", position: 3, position_tagged: true, looks_like_sub: false },
    { index: 3, title: "SW1", role: "SW1", position: 1, position_tagged: false, looks_like_sub: true },
    { index: 4, title: "SW2", role: "SW2", position: 1, position_tagged: false, looks_like_sub: true },
  ],
  suggestions: [
    { measurement: "L P3", role: "L", index: 0, position: 3 },
    { measurement: "R P3", role: "R", index: 1, position: 3 },
    { measurement: "C P3", role: "C", index: 2, position: 3 },
    { measurement: "SW1", role: "SW1", index: 3, position: null },
    { measurement: "SW2", role: "SW2", index: 4, position: null },
  ],
  problems: [],
};

/* ── Scenes ─────────────────────────────────────────────────────────────── */

export const TOUR_SCENES: TourScene[] = [
  // 시작
  {
    id: "start.new",
    step: "start",
    title: { ko: "새 프로젝트 대화상자", en: "New project dialog" },
    note: { ko: "프로젝트 없이 켜면 이 대화상자가 저절로 뜹니다.", en: "It opens by itself when the app starts without a project." },
    project: "none",
  },
  {
    id: "start.empty",
    step: "start",
    title: { ko: "프로젝트 없는 빈 화면", en: "Empty window without a project" },
    project: "none",
    ops: [click(ui("project.new.cancel"))],
  },
  {
    id: "start.tutorial-offer",
    step: "start",
    title: { ko: "튜토리얼 시작 대화상자", en: "Start tutorial dialog" },
    note: { ko: "도움말 패널의 튜토리얼 단추로 엽니다.", en: "Opened from the tutorial button in the help panel." },
    ops: [press("F1"), click(`.help-panel button${ui("help.intro.tutorial")}`)],
  },
  {
    id: "start.unsaved",
    step: "start",
    title: { ko: "저장하지 않은 변경 확인", en: "Unsaved changes prompt" },
    note: { ko: "변경이 있는 프로젝트에서 새로 만들기를 고른 경우입니다.", en: "Shown when choosing New while the project has unsaved changes." },
    setup: chair,
    ops: [click(ui("frame.project")), click(ui("frame.project.new"))],
  },
  {
    id: "start.open",
    step: "start",
    title: { ko: "프로젝트 열기 대화상자", en: "Open project dialog" },
    setup: async (mock) => {
      const settings = await mock.request({ method: "app_settings_get" });
      await mock.request({
        method: "app_settings_set",
        params: { settings: { ...settings, recent_projects: [PROJECT_PATH, "C:/Users/demo/Documents/SECS/studio-2.0.secs", "D:/Projects/cinema-9.1.6.secs"] } },
      });
    },
    ops: [press("Control+o")],
  },

  // 시스템
  { id: "system.stereo", step: "system", title: { ko: "2.0 새 프로젝트의 시스템", en: "System for a new 2.0 project" }, project: "stereo" },
  { id: "system.sample", step: "system", title: { ko: "7.1.4 · 서브우퍼 3, 측정 방식 고르기 전", en: "7.1.4 with 3 subwoofers, no measurement method yet" } },
  {
    id: "system.invalid",
    step: "system",
    title: { ko: "구성 오류와 막힌 단계", en: "Setup problems and blocked steps" },
    note: { ko: "R이 L과 같은 출력을 쓰고 Rs의 역할이 Ls와 겹칩니다.", en: "R uses the same output as L, and Rs repeats the role of Ls." },
    setup: async (mock) => {
      await mock.request({ method: "config_set_speaker", params: { index: 1, role: null, size: null, band_index: 0, out: 1, pol: null } });
      await mock.request({ method: "config_set_speaker", params: { index: 7, role: "Ls", size: null, band_index: null, out: null, pol: null } });
      // The mock's out_duplicate carries only `out`; the service (secs-project config.rs) also names both roles.
      reply(mock, "config_validate", [
        { code: "out_duplicate", params: { out: "1", first_role: "L", role: "R" } },
        { code: "role_duplicate", params: { role: "Ls" } },
      ]);
    },
  },
  {
    id: "system.path-native",
    step: "system",
    title: { ko: "측정 방식: 자체 측정", en: "Measurement method: built-in" },
    setup: (mock) => devices(mock),
    ops: choosePath("native", "system"),
  },
  {
    id: "system.path-rew",
    step: "system",
    title: { ko: "측정 방식: REW", en: "Measurement method: REW" },
    setup: (mock) => devices(mock, "rew"),
    ops: choosePath("rew", "system"),
  },
  {
    id: "system.path-mdat",
    step: "system",
    title: { ko: "측정 방식: mdat 파일만 가져오기", en: "Measurement method: mdat import only" },
    note: { ko: "연결·점검을 건너뛰는 방식이라 레일에 건너뜀이 보입니다.", en: "This method skips Connect & check, so the rail marks it skipped." },
    ops: choosePath("mdat", "system"),
  },
  { id: "system.expert", step: "system", title: { ko: "전문가 항목을 켠 시스템", en: "System with expert options shown" }, expert: true },

  // 연결·점검
  {
    id: "connect.ready",
    step: "connect",
    title: { ko: "장치를 고른 연결·점검", en: "Connect & check with devices chosen" },
    setup: (mock) => devices(mock),
    ops: [...choosePath("native"), tick(1)],
  },
  {
    id: "connect.level-running",
    step: "connect",
    title: { ko: "레벨 점검 진행 중", en: "Level check in progress" },
    note: { ko: "L을 마치고 R을 재는 중입니다.", en: "L is done and R is playing." },
    setup: (mock) => devices(mock),
    ops: [...choosePath("native"), click(ui("connect.level.play_all")), tick(7), reveal(ui("connect.level"))],
  },
  {
    id: "connect.level-done",
    step: "connect",
    title: { ko: "레벨 점검 완료", en: "Level check complete" },
    setup: (mock) => devices(mock),
    // 14 channels × 5 meter ticks each, with room to spare.
    ops: [...choosePath("native"), click(ui("connect.level.play_all")), tick(80), reveal(ui("connect.level"))],
  },
  {
    id: "connect.rew",
    step: "connect",
    title: { ko: "REW 연결", en: "REW connection" },
    setup: (mock) => devices(mock, "rew"),
    ops: [...choosePath("rew"), tick(1)],
  },
  {
    id: "connect.device-error",
    step: "connect",
    title: { ko: "장치 열기 실패", en: "Device failed to open" },
    setup: async (mock) => {
      await devices(mock);
      mock.failNext("level_monitor_start", "device_unavailable", "WASAPI: AUDCLNT_E_DEVICE_IN_USE (0x8889000A)");
    },
    ops: choosePath("native"),
  },
  {
    id: "connect.meter",
    step: "connect",
    title: { ko: "떠 있는 마이크 미터", en: "Floating mic meter" },
    note: { ko: "Ctrl+M으로 띄운 미터에 L의 분홍 잡음이 들어오는 중입니다.", en: "Opened with Ctrl+M while L plays pink noise." },
    setup: (mock) => devices(mock),
    ops: [...choosePath("native"), reveal(ui("connect.level")), click(ui("connect.level.play.L")), tick(2), press("Control+m")],
  },

  // 측정·가져오기
  {
    id: "measure.empty",
    step: "measure",
    title: { ko: "측정한 위치 없음", en: "No positions measured" },
    setup: async (mock) => {
      await devices(mock);
      await levels(mock);
      await chair(mock);
    },
    ops: choosePath("native", "measure"),
  },
  {
    id: "measure.running",
    step: "measure",
    title: { ko: "위치 측정 진행 중", en: "Measuring a position" },
    note: { ko: "작업 트레이에도 같은 진행이 보입니다.", en: "The job tray shows the same progress." },
    setup: async (mock) => {
      await devices(mock);
      await levels(mock);
      await chair(mock);
    },
    ops: [...choosePath("native", "measure"), click(ui("measure.position.start")), tick(3)],
  },
  {
    id: "measure.partial",
    step: "measure",
    title: { ko: "일부 위치만 측정됨", en: "Some positions measured" },
    setup: async (mock) => {
      await devices(mock);
      await levels(mock);
      await chair(mock);
      await measured(mock, 3);
    },
    ops: choosePath("native", "measure"),
  },
  {
    id: "measure.complete",
    step: "measure",
    title: { ko: "모든 위치 측정 완료", en: "All positions measured" },
    note: { ko: "데모 데이터를 불러온 청취 위치 하나의 배치입니다.", en: "The demo data for the main seat arrangement is loaded." },
    setup: inProgress,
    ops: choosePath("native", "measure"),
  },
  {
    id: "measure.low-snr",
    step: "measure",
    title: { ko: "낮은 S/N 경고", en: "Low S/N warning" },
    note: { ko: "위치 1을 잰 뒤 다시 골라 그 위치의 경고를 봅니다.", en: "Position 1 was measured and picked again to show its warning." },
    setup: async (mock) => {
      await devices(mock);
      await levels(mock);
      await chair(mock);
    },
    ops: [...choosePath("native", "measure"), click(ui("measure.position.start")), drain, click(ui("measure.positions.1"))],
  },
  {
    id: "measure.mdat",
    step: "measure",
    title: { ko: "mdat 가져오기 대화상자", en: "mdat import dialog" },
    note: {
      ko: "위치가 이름에 적힌 측정은 그 위치로, 적히지 않은 측정은 지금 위치(2)로 갑니다.",
      en: "Measurements named with a position go there; the others go to the current position (2).",
    },
    setup: async (mock) => {
      await devices(mock);
      await levels(mock);
      await chair(mock);
      await measured(mock, 1);
      reply(mock, "mdat_scan", MIXED_MDAT);
    },
    ops: [...choosePath("native", "measure"), click(ui("measure.mdat.load"))],
  },
  {
    id: "measure.no-path",
    step: "measure",
    title: { ko: "측정 방식이 없어 막힌 측정", en: "Measuring blocked without a method" },
    note: { ko: "새 프로젝트 대화상자에서 데모 방을 만든 직후입니다.", en: "Right after creating the demo room in the new project dialog." },
    project: "none",
    setup: (mock) => mock.queueDialog("dialog_save_project", PROJECT_PATH),
    ops: [click(ui("project.new.start.sample")), click(ui("project.new.create")), go("measure")],
  },

  // 보정
  {
    id: "calibrate.blocked",
    step: "calibrate",
    title: { ko: "측정이 없어 막힌 보정", en: "Correction blocked without measurements" },
    ops: [go("calibrate")],
  },
  {
    id: "calibrate.ready",
    step: "calibrate",
    title: { ko: "계산 전 보정 설정", en: "Correction settings before calculating" },
    setup: inProgress,
    ops: choosePath("native", "calibrate"),
  },
  {
    id: "calibrate.running",
    step: "calibrate",
    title: { ko: "보정 계산 진행 중", en: "Calculating correction" },
    setup: inProgress,
    ops: [...choosePath("native", "calibrate"), click(ui("calibrate.compute")), tick(4)],
  },
  {
    id: "calibrate.result",
    step: "calibrate",
    title: { ko: "보정 결과", en: "Correction result" },
    setup: async (mock) => {
      await inProgress(mock);
      await calibrated(mock);
    },
    ops: choosePath("native", "calibrate"),
  },
  {
    id: "calibrate.result-alt",
    step: "calibrate",
    title: { ko: "보정 결과: BA 탭", en: "Correction result: BA tab" },
    note: { ko: "서브 합 그래프와 서브마다의 정렬 값입니다.", en: "The sub sum graph and each subwoofer's alignment values." },
    setup: async (mock) => {
      await inProgress(mock);
      await calibrated(mock);
    },
    ops: [...choosePath("native", "calibrate"), click(ui("calibrate.tab.ba")), reveal(ui("calibrate.graph"))],
  },
  {
    id: "calibrate.sub",
    step: "calibrate",
    title: { ko: "서브우퍼 설정 편집", en: "Editing subwoofer settings" },
    note: { ko: "서브 정렬의 고급을 펴고 고정 방식으로 바꾼 SW2를 고른 상태입니다.", en: "BA advanced is open on SW2, switched to the fixed method." },
    setup: async (mock) => {
      await inProgress(mock);
      await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "sub_mode", role: "SW2", mode: "fixed" } } });
      await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "sub", role: "SW2", patch: { gain_db: -2.5, delay_ms: 3.2, lp_hz: 90 } } } });
    },
    ops: [...choosePath("native", "calibrate"), click(ui("calibrate.ba.advanced")), click(ui("calibrate.ba.edit.SW2")), reveal(`.ui-segmented-field:has(${ui("calibrate.ba.edit")})`)],
  },
  {
    id: "calibrate.xo",
    step: "calibrate",
    title: { ko: "XO 추천 받음", en: "XO recommendation received" },
    setup: inProgress,
    ops: [...choosePath("native", "calibrate"), click(ui("calibrate.xo.recommend")), drain, reveal(ui("calibrate.settings.xo"))],
  },
  {
    id: "calibrate.stale",
    step: "calibrate",
    title: { ko: "다시 계산 필요", en: "Recalculation needed" },
    note: { ko: "결과를 낸 뒤 부스트 상한을 바꿨습니다.", en: "The boost limit changed after the result was calculated." },
    setup: async (mock) => {
      await inProgress(mock);
      await calibrated(mock);
      await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "boost_max", db: 6 } } });
    },
    ops: choosePath("native", "calibrate"),
  },
  {
    id: "calibrate.failed",
    step: "calibrate",
    title: { ko: "보정 계산 실패", en: "Correction calculation failed" },
    setup: async (mock) => {
      await inProgress(mock);
      mock.failJobs("calibrate", "engine_input", "position 1: SW3 impulse response is empty");
    },
    ops: [...choosePath("native", "calibrate"), click(ui("calibrate.compute")), drain],
  },

  // 내보내기
  {
    id: "export.blocked",
    step: "export",
    title: { ko: "결과가 없어 막힌 내보내기", en: "Export blocked without a result" },
    setup: inProgress,
    ops: choosePath("native", "export"),
  },
  {
    id: "export.ready",
    step: "export",
    title: { ko: "내보내기 전", en: "Before exporting" },
    setup: async (mock) => {
      await inProgress(mock);
      await calibrated(mock);
    },
    ops: choosePath("native", "export"),
  },
  {
    id: "export.running",
    step: "export",
    title: { ko: "내보내기 진행 중", en: "Exporting" },
    setup: async (mock) => {
      await inProgress(mock);
      await calibrated(mock);
    },
    ops: [...choosePath("native", "export"), click(`.step-header ${ui("export.run.camilla_dsp")}`)],
  },
  {
    id: "export.done",
    step: "export",
    title: { ko: "내보내기 완료", en: "Export complete" },
    setup: async (mock) => {
      await inProgress(mock);
      await calibrated(mock);
    },
    ops: [...choosePath("native", "export"), click(`.step-header ${ui("export.run.camilla_dsp")}`), drain],
  },
  {
    id: "export.filters",
    step: "export",
    title: { ko: "DSP에 넣을 필터", en: "Filters for the DSP" },
    note: {
      ko: "서브가 있는 결과라 필터 값 텍스트는 꺼지고 그 이유가 줄 안에 보입니다.",
      en: "The result has subs, so the filter text is off and its row says why.",
    },
    setup: async (mock) => {
      await inProgress(mock);
      await calibrated(mock);
    },
    ops: [...choosePath("native", "export"), reveal(ui("export.files"))],
  },
  {
    id: "export.eqapo-done",
    step: "export",
    title: { ko: "Equalizer APO 설정 저장 완료", en: "Equalizer APO configuration saved" },
    note: {
      ko: "결과 줄의 파일 이름은 모의 서비스의 자리표시이고, 그 아래가 복사 안내입니다.",
      en: "The outcome's file names are the mock service's placeholders; where to copy the files sits under it.",
    },
    setup: async (mock) => {
      await inProgress(mock);
      await calibrated(mock);
    },
    ops: [...choosePath("native", "export"), reveal(ui("export.kind.filter_text")), click(ui("export.run.equalizer_apo")), drain],
  },
  {
    id: "export.mimo-blocked",
    step: "export",
    title: { ko: "MIMO를 넣은 결과의 필터", en: "Filters for a result with MIMO" },
    note: {
      ko: "세 위치를 측정하고 MIMO를 계산한 결과라 필터 값 텍스트와 Equalizer APO가 꺼집니다.",
      en: "With three positions measured and MIMO calculated, the filter text and Equalizer APO are off.",
    },
    setup: async (mock) => {
      await devices(mock);
      await levels(mock);
      await chair(mock);
      await measured(mock, 3);
      // Switched on before the correction: a settings edit afterwards would make the result stale.
      await mock.request({ method: "calibration_settings_edit", params: { edit: { kind: "mimo_on", on: true } } });
      await calibrated(mock);
      await mimo(mock);
    },
    ops: [...choosePath("native", "export"), reveal(ui("export.files"))],
  },
  {
    id: "export.failed",
    step: "export",
    title: { ko: "내보내기 실패", en: "Export failed" },
    setup: async (mock) => {
      await inProgress(mock);
      await calibrated(mock);
      mock.failJobs("export", "export_failed", "C:/Users/demo/Documents/SECS_export/camilladsp: access is denied (os error 5)");
    },
    ops: [...choosePath("native", "export"), click(`.step-header ${ui("export.run.camilla_dsp")}`), drain],
  },

  // 앱 틀
  { id: "frame.help", step: "frame", title: { ko: "도움말 패널", en: "Help panel" }, ops: [press("F1")] },
  {
    id: "frame.jobs",
    step: "frame",
    title: { ko: "펼친 작업 트레이", en: "Expanded job tray" },
    note: { ko: "진행 중인 보정 계산 하나와 완료·실패·취소 기록입니다.", en: "One correction running, with completed, failed and canceled entries." },
    setup: async (mock) => {
      await inProgress(mock);
      mock.failJobs("recommend_xo", "engine_input", "front: no crossover candidate between 60 and 120 Hz");
    },
    ops: [
      ...choosePath("native", "calibrate"),
      // Completed, failed, canceled, then one left running.
      click(ui("calibrate.compute")),
      drain,
      click(ui("calibrate.xo.recommend")),
      drain,
      click(ui("calibrate.compute")),
      tick(2),
      click(ui("jobs.cancel.calibrate")),
      drain,
      click(ui("calibrate.compute")),
      tick(3),
      reveal(ui("calibrate.verdict")),
      click(ui("jobs.history.toggle")),
    ],
  },
  { id: "frame.project-menu", step: "frame", title: { ko: "프로젝트 메뉴", en: "Project menu" }, ops: [click(ui("frame.project"))] },
  { id: "frame.settings", step: "frame", title: { ko: "설정 패널", en: "Settings panel" }, ops: [click(ui("frame.settings"))] },

  // 튜토리얼 (tutorialLive.tsx와 같은 시작점)
  { id: "tutorial.system", step: "tutorial", title: { ko: "튜토리얼: 시스템", en: "Tutorial: System" }, tutorialStart: "system.output" },
  { id: "tutorial.connect", step: "tutorial", title: { ko: "튜토리얼: 연결·점검", en: "Tutorial: Connect & check" }, tutorialStart: "connect.retry" },
  { id: "tutorial.measure", step: "tutorial", title: { ko: "튜토리얼: 측정·가져오기", en: "Tutorial: Measure & import" }, tutorialStart: "measure.sample" },
  {
    id: "tutorial.calibrate",
    step: "tutorial",
    title: { ko: "튜토리얼: 보정", en: "Tutorial: Correction" },
    tutorialStart: "calibrate.compute",
    setup: (mock) => measured(mock),
  },
  {
    id: "tutorial.export",
    step: "tutorial",
    title: { ko: "튜토리얼: 내보내기", en: "Tutorial: Export" },
    tutorialStart: "export.mdat",
    setup: async (mock) => {
      await measured(mock);
      await calibrated(mock);
    },
  },
];
