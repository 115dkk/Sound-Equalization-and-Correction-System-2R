// The tour's wall clock. The job tray is the only reader of `Date.now()`
// (elapsed, time left, the history's times); in the tour that time stands
// still at TOUR_EPOCH and moves only when a scene ticks or drains the mock,
// so a running job reads the same "0:12 elapsed · about 0:44 left" on every
// capture. Installed by the tour page only; importing this has no effect.

/** 09:41 in Seoul (scripts/screen-tour.mjs captures in that time zone). */
const TOUR_EPOCH = Date.parse("2026-10-15T09:41:00+09:00");

let offset = 0;
let moved = false;

export function installTourClock() {
  Date.now = () => TOUR_EPOCH + offset;
}

/** Moves the tour clock forward by `ms` (a mock tick stands for a few seconds of work). */
export function advanceClock(ms: number) {
  offset += ms;
  moved = true;
}

/** Whether the clock moved since the last call; the tray's display clock catches up within a second. */
export function clockMoved(): boolean {
  const was = moved;
  moved = false;
  return was;
}
