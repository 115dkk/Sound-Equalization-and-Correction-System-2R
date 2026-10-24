// @vitest-environment happy-dom
import { fireEvent, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { renderWith } from "../../test/render";
import { ArrangementMap, nextPosition } from "./ArrangementMap";
import { arrangementPoints } from "./geometry";
import { HIT_R, RoomMap, hitCell } from "./RoomMap";

describe("arrangement map", () => {
  it("copies the upstream arrangements", () => {
    expect(arrangementPoints(1)).toEqual([{ x: 0, y: 0, h: 0 }]);
    expect(arrangementPoints(9)).toHaveLength(9);
    expect(arrangementPoints(13)[11]).toEqual({ x: 0, y: 70, h: 1 });
    expect(arrangementPoints(17)[16]).toEqual({ x: -55, y: -50, h: -1 });
  });

  it("moves the selection like upstream: first or last without one, ±1 inside the range", () => {
    expect(nextPosition(null, "ArrowRight", 9)).toBe(1);
    expect(nextPosition(null, "ArrowLeft", 9)).toBe(9);
    expect(nextPosition(4, "ArrowDown", 9)).toBe(5);
    expect(nextPosition(4, "ArrowUp", 9)).toBe(3);
    expect(nextPosition(9, "ArrowRight", 9)).toBeNull();
    expect(nextPosition(1, "ArrowLeft", 9)).toBeNull();
    expect(nextPosition(5, "Home", 9)).toBe(1);
    expect(nextPosition(5, "End", 9)).toBe(9);
    expect(nextPosition(5, "Enter", 9)).toBeNull();
  });

  function Harness({ onSelect }: { onSelect: (n: number) => void }) {
    const [current, setCurrent] = useState<number | null>(4);
    return (
      <ArrangementMap
        id="map"
        count={9}
        done={[1, 2, 3]}
        current={current}
        onSelect={(n) => {
          setCurrent(n);
          onSelect(n);
        }}
      />
    );
  }

  it("is one tab stop whose arrow keys move the selected position", () => {
    const onSelect = vi.fn();
    renderWith(<Harness onSelect={onSelect} />);
    const map = screen.getByRole("listbox");
    expect(map.getAttribute("tabindex")).toBe("0");
    expect(map.getAttribute("aria-activedescendant")).toBe("map-pos-4");
    fireEvent.keyDown(map, { key: "ArrowRight" });
    expect(onSelect).toHaveBeenLastCalledWith(5);
    expect(map.getAttribute("aria-activedescendant")).toBe("map-pos-5");
    fireEvent.keyDown(map, { key: "End" });
    fireEvent.keyDown(map, { key: "ArrowRight" });
    expect(onSelect).toHaveBeenLastCalledWith(9);
    expect(onSelect).toHaveBeenCalledTimes(2);
    const options = screen.getAllByRole("option");
    expect(options).toHaveLength(9);
    expect(options[0]!.getAttribute("aria-label")).toContain("완료");
    fireEvent.click(options[1]!);
    expect(onSelect).toHaveBeenLastCalledWith(2);
  });

  it("is a picture, not a control, without onSelect", () => {
    renderWith(<ArrangementMap id="map" count={13} />);
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(screen.getByRole("img").getAttribute("aria-label")).toBe("측정 위치 배치도, 위치 13곳");
  });
});

describe("room map", () => {
  it("selects speakers with the arrow keys and lists speakers without a place", () => {
    const onSelect = vi.fn();
    renderWith(
      <RoomMap
        id="room"
        speakers={[
          { role: "L", outputs: [1] },
          { role: "R", outputs: [2] },
          { role: "SW1", outputs: [3] },
          { role: "Bass shaker", outputs: [9] },
        ]}
        selected={0}
        onSelect={onSelect}
      />,
    );
    const map = screen.getByRole("listbox");
    fireEvent.keyDown(map, { key: "ArrowRight" });
    expect(onSelect).toHaveBeenLastCalledWith(1);
    expect(screen.getAllByRole("option")).toHaveLength(3);
    expect(screen.getByRole("option", { name: "SW1, 서브우퍼, 출력 3" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "역할 미지정 스피커, 출력 9" }));
    expect(onSelect).toHaveBeenLastCalledWith(3);
  });

  /** Ray casting: whether (x, y) lies inside the polygon. */
  const inside = (polygon: readonly (readonly [number, number])[], [x, y]: readonly [number, number]) => {
    let hit = false;
    for (let i = 0, j = polygon.length - 1; i < polygon.length; j = i++) {
      const [xi, yi] = polygon[i]!;
      const [xj, yj] = polygon[j]!;
      if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) hit = !hit;
    }
    return hit;
  };
  const roles = ["L", "R", "C", "SW1", "SW2", "SW3", "SW4", "SW5", "Ls", "Rs", "Lb", "Rb", "Lw", "Rw", "Tfl", "Tfr", "Tml", "Tmr", "Tbl", "Tbr", "Hfl", "Hfr", "Hbl", "Hbr"];

  it("presses every speaker through a hit circle 40 px across without drawing it", () => {
    renderWith(<RoomMap id="room" speakers={roles.map((role, i) => ({ role, outputs: [i + 1] }))} selected={2} onSelect={() => {}} />);
    const hits = document.querySelectorAll(".map-speaker__hit");
    expect(hits).toHaveLength(roles.length);
    for (const hit of hits) expect(Number(hit.getAttribute("r"))).toBeGreaterThanOrEqual(20);
    // C and SW5 sit 30 px apart: each circle is clipped to its own half; L has no neighbour that close.
    const clip = (role: string) => document.querySelector(`[data-ui="room.speaker.${role}"] .map-speaker__hit`)?.getAttribute("clip-path");
    expect(clip("C")).toBe("url(#room-spk-2-hit)");
    expect(clip("SW5")).toBe("url(#room-spk-7-hit)");
    expect(clip("L")).toBeNull();
    expect(document.getElementById("room-spk-2-hit")?.querySelector("polygon")).not.toBeNull();
  });

  it("gives the overlap of two close speakers' hit circles to the nearer speaker", () => {
    const c = [210, 65.6] as const;
    const sw5 = [210, 35.2] as const;
    const cCell = hitCell(c, [c, sw5])!;
    const swCell = hitCell(sw5, [c, sw5])!;
    // Inside both circles: 2 px towards C from the halfway line belongs to C only, and the other way to SW5.
    expect(inside(cCell, [210, 52.4])).toBe(true);
    expect(inside(swCell, [210, 52.4])).toBe(false);
    expect(inside(swCell, [210, 48.4])).toBe(true);
    expect(inside(cCell, [210, 48.4])).toBe(false);
    // Away from the neighbour each keeps its whole radius.
    expect(inside(cCell, [210, c[1] + HIT_R - 1])).toBe(true);
    expect(inside(cCell, [c[0] + HIT_R - 1, c[1]])).toBe(true);
    expect(hitCell([100, 100], [[100, 100], [100, 140]])).toBeNull();
  });
});
