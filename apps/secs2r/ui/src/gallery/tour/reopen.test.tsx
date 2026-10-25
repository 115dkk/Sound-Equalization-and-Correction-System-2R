// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, beforeAll, expect, it } from "vitest";
import { App } from "../../App";
import { createMockService } from "../../ipc/mock";
import { installFakeCanvas } from "../../test/canvas";
import { PROJECT_PATH, TOUR_SCENES, type TourOp } from "./scenes";

beforeAll(() => installFakeCanvas());
afterEach(cleanup);

/** Presses and clicks the way scripts/screen-tour.mjs does; page ops (ticks, scrolling) need a real layout and are skipped. */
async function perform(op: TourOp) {
  if (op.kind === "press") {
    const [modifier, key] = op.key.includes("+") ? op.key.split("+") : [null, op.key];
    await act(async () => { fireEvent.keyDown(window, { key, ctrlKey: modifier === "Control" }); });
  } else if (op.kind === "click") {
    await waitFor(() => expect(document.querySelector(op.selector), op.selector).not.toBeNull());
    await act(async () => { fireEvent.click(document.querySelector(op.selector)!); });
  }
}

// The scenes that save and reopen the project show it as it was saved: nothing the scene does on the way edits it.
it.each(["export.stale-cache", "export.recalibrating"])("%s keeps the reopened project saved", async (id) => {
  const scene = TOUR_SCENES.find((s) => s.id === id)!;
  const mock = createMockService({ project: null, tickMs: 0 });
  await mock.request({ method: "project_new", params: { path: PROJECT_PATH, preset: "7.1.4", sub_count: 3, sample: true, tutorial: false } });
  await scene.setup!(mock);
  const rendered = render(<App ipc={mock} />);
  try {
    await waitFor(() => expect(document.querySelector('[data-ui="frame"]')).not.toBeNull());
    for (const op of scene.ops ?? []) await perform(op);
    await waitFor(() => expect(document.querySelector('[data-ui="export.kind.before_after_mdat"]')).not.toBeNull());
    const edits = mock.calls.slice(mock.calls.map((r) => r.method).lastIndexOf("project_open") + 1)
      .filter((r) => ["measure_configure", "calibration_settings_edit", "config_set_speaker", "layout_set"].includes(r.method));
    expect(edits).toEqual([]);
    expect((await mock.request({ method: "project_info" })).dirty).toBe(false);
  } finally { rendered.unmount(); mock.dispose(); }
}, 15000);
