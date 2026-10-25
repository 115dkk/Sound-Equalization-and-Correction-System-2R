import { describe, expect, it } from "vitest";
import { translate } from "../../i18n";
import { recordSources, sourceSummary } from "./format";

const t = translate.bind(null, "ko");

describe("measurement sources", () => {
  it("names one direct measurement and one imported file from service sources", () => {
    const records = [
      { position: 1, done: true, roles: ["L"], sources: { L: "measured" } },
      { position: 2, done: true, roles: ["L"], sources: { L: "file.mdat" } },
    ];
    expect(sourceSummary(t, records)).toBe("측정함 (위치 1) · file.mdat (위치 2)");
    expect(recordSources({ ...records[0]!, sources: { L: "" } })).toEqual([""]);
  });
});
