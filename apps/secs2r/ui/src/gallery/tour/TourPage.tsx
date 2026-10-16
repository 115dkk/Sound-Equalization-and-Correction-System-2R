// The screen tour's page: gallery.html?tour=<scene id>&theme=light|dark&lang=ko|en
// mounts the real App the way main.tsx does (root: theme, accent and
// language on <html>) on the mock service with no automatic tick, and
// leaves `window.__tour` for scripts/screen-tour.mjs to drive. With an
// empty `tour=` it lists the scenes instead (`window.__tourScenes`).
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "../../App";
import type { Prefs } from "../../app/prefs";
import { makeClient } from "../../ipc/client";
import type { Ipc } from "../../ipc/context";
import { createMockService, type MockService } from "../../ipc/mock";
import { clockMoved, installTourClock } from "./clock";
import { PROJECT_PATH, TOUR_SCENES, type TourOp, type TourScene } from "./scenes";

export type TourSceneInfo = Pick<TourScene, "id" | "step" | "title" | "note">;
type SerializedOp = { kind: "press"; key: string } | { kind: "click" | "hover"; selector: string } | { kind: "page" };

export type TourHandle = {
  scene: { id: string; opCount: number; ops: TourOp["kind"][] };
  /** Runs page op `i` and waits for the window to settle. */
  run: (i: number) => Promise<void>;
  /** Op `i` as data (the key or selector for press, click and hover). */
  op: (i: number) => SerializedOp;
  /** Resolves once the DOM has stayed still for a moment. */
  settle: () => Promise<void>;
};

declare global {
  interface Window {
    __tour?: TourHandle;
    __tourScenes?: TourSceneInfo[];
  }
}

const sceneInfo = (scene: TourScene): TourSceneInfo => ({ id: scene.id, step: scene.step, title: scene.title, ...(scene.note ? { note: scene.note } : {}) });

const serialize = (op: TourOp): SerializedOp => (op.kind === "page" ? { kind: "page" } : op.kind === "press" ? { kind: "press", key: op.key } : { kind: op.kind, selector: op.selector });

const frame = () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

/** Waits until `selector` matches (rAF polling), or throws after `timeoutMs`. */
async function present(selector: string, timeoutMs = 8000) {
  const start = performance.now();
  while (!document.querySelector(selector)) {
    if (performance.now() - start > timeoutMs) throw new Error(`tour: ${selector} did not appear within ${timeoutMs} ms`);
    await frame();
  }
}

const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

/**
 * Resolves after the DOM has had no mutation for `quietMs`, or after `maxMs`.
 * The mock has no timer, so the only work left after an op is the promise
 * chains it started and the renders they cause. When the tour clock moved,
 * it first gives the job tray's one-second display clock time to catch up.
 */
async function settle(quietMs = 150, maxMs = 3000): Promise<void> {
  if (clockMoved()) await sleep(1100);
  return new Promise((resolve) => {
    let quiet: ReturnType<typeof setTimeout>;
    const done = () => {
      observer.disconnect();
      clearTimeout(quiet);
      clearTimeout(cap);
      resolve();
    };
    const observer = new MutationObserver(() => {
      clearTimeout(quiet);
      quiet = setTimeout(done, quietMs);
    });
    observer.observe(document.documentElement, { subtree: true, childList: true, attributes: true, characterData: true });
    quiet = setTimeout(done, quietMs);
    const cap = setTimeout(done, maxMs);
  });
}

/** The scene's starting project, made the way the new-project dialog makes it. */
async function openProject(mock: MockService, scene: TourScene) {
  const project = scene.project ?? "sample";
  if (project === "none") return;
  const tutorial = scene.tutorialStart !== undefined;
  await mock.request({
    method: "project_new",
    params:
      project === "stereo"
        ? { path: PROJECT_PATH, preset: "2.0", sub_count: 0, sample: false, tutorial: false }
        : // The tutorial project is never saved (upstream and App's restart make it the same way).
          { path: tutorial ? "" : PROJECT_PATH, preset: "7.1.4", sub_count: 3, sample: true, tutorial },
  });
}

function TourIndex() {
  const variants = ["light.ko", "dark.ko", "light.en", "dark.en"];
  return (
    <div className="gal-page">
      <header className="gal-toolbar">
        <strong className="gal-toolbar__title">SECS 2R screen tour · {TOUR_SCENES.length} scenes</strong>
      </header>
      <ol className="gal-stack">
        {TOUR_SCENES.map((scene) => (
          <li key={scene.id} className="gal-row">
            <code>{scene.id}</code>
            <span>{scene.title.ko}</span>
            <span className="gal-note">{scene.title.en}</span>
            {variants.map((variant) => {
              const [theme, lang] = variant.split(".");
              return (
                <a key={variant} href={`?tour=${scene.id}&theme=${theme}&lang=${lang}`}>
                  {variant}
                </a>
              );
            })}
          </li>
        ))}
      </ol>
    </div>
  );
}

export function mountTour(container: HTMLElement, params: URLSearchParams) {
  const html = document.documentElement;
  const id = params.get("tour") ?? "";
  if (id === "") {
    window.__tourScenes = TOUR_SCENES.map(sceneInfo);
    createRoot(container).render(<TourIndex />);
    html.dataset.tourReady = "list";
    return;
  }
  const scene = TOUR_SCENES.find((s) => s.id === id);
  if (!scene) {
    html.dataset.tourReady = "missing";
    console.error(`screen tour: no scene "${id}"`);
    return;
  }
  installTourClock();
  void (async () => {
    const mock = createMockService({ project: null, tickMs: 0 });
    await openProject(mock, scene);
    await scene.setup?.(mock);
    const ops = scene.ops ?? [];
    window.__tour = {
      scene: { id: scene.id, opCount: ops.length, ops: ops.map((op) => op.kind) },
      op: (i) => {
        const op = ops[i];
        if (!op) throw new Error(`screen tour: ${scene.id} has no op ${i}`);
        return serialize(op);
      },
      run: async (i) => {
        const op = ops[i];
        if (op?.kind !== "page") throw new Error(`screen tour: op ${i} of ${scene.id} is not a page op`);
        await op.run({ mock });
        await settle();
      },
      settle: () => settle(),
    };
    // Requests go through `mock.invoke` at call time, so a setup may wrap it.
    const ipc: Ipc = { request: makeClient((command, args) => mock.invoke(command, args)), events: mock.events };
    const prefs: Prefs = {
      theme: params.get("theme") === "dark" ? "dark" : "light",
      locale: params.get("lang") === "en" ? "en" : "ko",
      expert: scene.expert ?? false,
      accent: "default",
    };
    createRoot(container).render(
      <StrictMode>
        <App ipc={ipc} root initialPrefs={prefs} {...(scene.tutorialStart ? { tutorialStart: scene.tutorialStart } : {})} />
      </StrictMode>,
    );
    await present('[data-ui="frame"]');
    if (scene.tutorialStart) await present('[data-ui="tutorial.coach"]');
    await settle();
    html.dataset.tourReady = "1";
  })().catch((error: unknown) => {
    html.dataset.tourReady = "error";
    console.error(`screen tour: ${scene.id} failed to start: ${error instanceof Error ? error.stack ?? error.message : String(error)}`);
  });
}
