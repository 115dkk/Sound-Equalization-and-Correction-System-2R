// The step screens' gallery sections (docs/packets/M5-screens-common.md).
// Each screen packet adds one import and one entry; ids are
// `screen.<step>.<state>` so CI screenshots them one by one.
import type { SectionDef } from "../gallery/Specimen";
import { calibrateSpecimens } from "./calibrate/specimens";

export const screenSections: SectionDef[] = [{ id: "screen.calibrate", title: "Screen · 보정", specimens: calibrateSpecimens }];
