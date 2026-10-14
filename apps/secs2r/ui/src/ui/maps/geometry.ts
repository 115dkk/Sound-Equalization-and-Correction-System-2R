// Map geometry fixtures, copied from upstream SECS 2 (GPL-3.0, the same
// licence as SECS 2R) at the commit in UPSTREAM.toml (Beta 1.2.0,
// 60ac53ad3a3d9304da115e5dda3183a475d2d7dd):
//   - src/secs/wizard/layout_map.py: arr_points(), ARRANGEMENTS, the 1.55 /
//     1.9 px-per-cm scale, the chair and sofa outlines;
//   - src/secs/model.py: ROLE_POSITIONS, ROLE_CODES, CHINFO kinds, ROLES order, PRESETS.
// The service will send positions and geometry later (P26 `layout_get`); the
// maps take everything as props, so these stay as fixtures and test data.

/** A microphone position in cm around the listening head: x right +, y toward the screen +, h = ±20 cm. */
export type MicPoint = { x: number; y: number; h: -1 | 0 | 1 };

/** Arrangements by position count (upstream ARRANGEMENTS). */
export const ARRANGEMENT_COUNTS = [1, 9, 13, 17] as const;
export type ArrangementCount = (typeof ARRANGEMENT_COUNTS)[number];

const RING1: readonly MicPoint[] = [
  { x: 35, y: 0, h: 0 },
  { x: -35, y: 0, h: 0 },
  { x: 0, y: 35, h: 0 },
  { x: 0, y: -30, h: 0 },
  { x: 25, y: 25, h: 1 },
  { x: -25, y: 25, h: 1 },
  { x: 25, y: -20, h: -1 },
  { x: -25, y: -20, h: -1 },
];
const RING2: readonly MicPoint[] = [
  { x: 70, y: 0, h: 0 },
  { x: -70, y: 0, h: 0 },
  { x: 0, y: 70, h: 1 },
  { x: 0, y: -55, h: 0 },
];
const RING3: readonly MicPoint[] = [
  { x: 55, y: 50, h: 0 },
  { x: -55, y: 50, h: 0 },
  { x: 55, y: -50, h: -1 },
  { x: -55, y: -50, h: -1 },
];

/** Upstream arr_points(n): position 1 is the listening position at ear height. */
export function arrangementPoints(count: number): MicPoint[] {
  const points: MicPoint[] = [{ x: 0, y: 0, h: 0 }, ...RING1];
  if (count >= 13) points.push(...RING2);
  if (count >= 17) points.push(...RING3);
  return points.slice(0, count);
}

export type RoleKind = "main" | "sub" | "top" | "height";

/** Upstream ROLES order (screen lists and measuring order). */
export const ROLES = [
  "L", "R", "C",
  "SW1", "SW2", "SW3", "SW4", "SW5", "SW6", "SW7", "SW8",
  "Ls", "Rs", "Lb", "Rb", "Lw", "Rw",
  "Tfl", "Tfr", "Tml", "Tmr", "Tbl", "Tbr",
  "Hfl", "Hfr", "Hbl", "Hbr",
] as const;

/** Relative room position (0–1, screen at the top) per role (upstream ROLE_POSITIONS). */
export const ROOM_POSITIONS: Readonly<Record<string, readonly [number, number]>> = {
  L: [0.27, 0.18], R: [0.73, 0.18], C: [0.5, 0.12],
  SW1: [0.16, 0.4], SW2: [0.84, 0.4], SW3: [0.16, 0.78], SW4: [0.84, 0.78],
  SW5: [0.5, 0.04], SW6: [0.5, 0.97], SW7: [0.04, 0.5], SW8: [0.96, 0.5],
  Ls: [0.1, 0.62], Rs: [0.9, 0.62], Lb: [0.22, 0.93], Rb: [0.78, 0.93],
  Lw: [0.06, 0.34], Rw: [0.94, 0.34],
  Tfl: [0.36, 0.3], Tfr: [0.64, 0.3], Tml: [0.36, 0.5], Tmr: [0.64, 0.5],
  Tbl: [0.36, 0.84], Tbr: [0.64, 0.84],
  Hfl: [0.14, 0.08], Hfr: [0.86, 0.08], Hbl: [0.07, 0.86], Hbr: [0.93, 0.86],
};

/** Dolby codes shown on screen where they differ from the role id (upstream ROLE_CODES). */
const ROLE_CODES: Readonly<Record<string, string>> = {
  Lb: "Lrs", Rb: "Rrs",
  Tfl: "Ltf", Tfr: "Rtf", Tml: "Ltm", Tmr: "Rtm", Tbl: "Ltr", Tbr: "Rtr",
  Hfl: "Lfh", Hfr: "Rfh", Hbl: "Lrh", Hbr: "Rrh",
};

export const roleCode = (role: string): string => ROLE_CODES[role] ?? role;

export function roleKind(role: string): RoleKind {
  if (/^SW[1-8]$/.test(role)) return "sub";
  if (/^T(f|m|b)[lr]$/.test(role)) return "top";
  if (/^H(f|b)[lr]$/.test(role)) return "height";
  return "main";
}

export const isSubRole = (role: string): boolean => roleKind(role) === "sub";

/** Sorts roles into ROLES order; unknown (custom) roles keep their order at the end. */
export function orderRoles(roles: readonly string[]): string[] {
  const index = (role: string) => {
    const i = (ROLES as readonly string[]).indexOf(role);
    return i < 0 ? ROLES.length : i;
  };
  return roles.map((role, i) => ({ role, i })).sort((a, b) => index(a.role) - index(b.role) || a.i - b.i).map((x) => x.role);
}

/** Speaker presets used by fixtures (upstream model.PRESETS). */
export const PRESET_ROLES: Readonly<Record<string, readonly string[]>> = {
  "2.0": ["L", "R"],
  "2.1": ["L", "R", "SW1"],
  "2.2": ["L", "R", "SW1", "SW2"],
  "5.1": ["L", "R", "C", "SW1", "Ls", "Rs"],
  "7.1.4": ["L", "R", "C", "SW1", "Ls", "Rs", "Lb", "Rb", "Tfl", "Tfr", "Tbl", "Tbr"],
  "9.1.6": ["L", "R", "C", "SW1", "Ls", "Rs", "Lb", "Rb", "Lw", "Rw", "Tfl", "Tfr", "Tml", "Tmr", "Tbl", "Tbr"],
};
