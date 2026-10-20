// @vitest-environment happy-dom
import { fireEvent, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { STEPS, type StepId } from "../../app/model";
import { isMac } from "../../app/shortcuts";
import { catalogs, isMessageKey, translate } from "../../i18n";
import { renderWith } from "../../test/render";
import { HELP_TABS, MDAT_CHANNEL_ROWS } from "./content";
import { HelpPanel, countMatches, tabTexts } from "./HelpPanel";
import type { HelpActions, HelpBlock } from "./types";

/** Every id docs/M5-reach.md cites for the help panel. */
const REACH_IDS = [
  "help",
  "help.step",
  "help.close",
  "help.search",
  "help.search.next",
  "help.tabs",
  "help.tab.intro",
  "help.tab.steps",
  "help.tab.checks",
  "help.tab.trouble",
  "help.tab.real",
  "help.tab.mdat",
  "help.tab.camilla",
  "help.intro.tutorial",
] as const;

function actions() {
  return { close: vi.fn<HelpActions["close"]>(), startTutorial: vi.fn<HelpActions["startTutorial"]>() };
}

const keysOf = (block: HelpBlock): string[] => {
  switch (block.kind) {
    case "lead":
    case "heading":
      return [block.text];
    case "steps":
      return block.items.flatMap((item) => item.see ? [item.do, item.see] : [item.do]);
    case "pairs":
      return block.items.flatMap((item) => [item.term, item.text]);
    case "channels":
      return MDAT_CHANNEL_ROWS.map((row) => row.group);
    case "tutorial":
      return [];
    default:
      return [...block.items];
  }
};

const ui = (id: string) => document.querySelector<HTMLElement>(`[data-ui="${id}"]`);

describe("도움말", () => {
  it("has every step's text in both catalogs, and every help key is used", () => {
    const used = new Set<string>();
    for (const step of STEPS) for (const tab of HELP_TABS[step]) for (const block of tab.blocks) for (const key of keysOf(block)) used.add(key);
    for (const key of used) expect(isMessageKey(key), key).toBe(true);
    const chrome = /^help\.(panel|title|step|close|search|keys|tabs|tab|intro\.tutorial|measure\.mdat\.table)/;
    const unused = Object.keys(catalogs.ko).filter((key) => key.startsWith("help.") && !used.has(key) && !chrome.test(key));
    expect(unused).toEqual([]);
  });

  it("gives each step its tabs: 처음이라면 on 시스템, mdat 가져오기 on 측정·가져오기, CamillaDSP on 내보내기", () => {
    const tabs = (step: StepId) => HELP_TABS[step].map((tab) => tab.id);
    expect(tabs("system")).toEqual(["intro", "steps", "checks", "trouble", "real"]);
    expect(tabs("connect")).toEqual(["steps", "checks", "trouble", "real"]);
    expect(tabs("measure")).toEqual(["steps", "checks", "trouble", "real", "mdat"]);
    expect(tabs("calibrate")).toEqual(["steps", "checks", "trouble", "real"]);
    expect(tabs("export")).toEqual(["steps", "checks", "trouble", "real", "camilla"]);
  });

  it("keeps the SNR threshold at 20 dB (U6)", () => {
    const all = STEPS.flatMap((step) => HELP_TABS[step].flatMap((tab) => tabTexts((key, params) => translate("ko", key, params), tab))).join("\n");
    expect(all).toContain("20 dB보다 낮다는 경고");
    expect(all).not.toMatch(/40 dB/);
  });

  it("renders every step and tab in both languages", () => {
    for (const locale of ["ko", "en"] as const) {
      for (const step of STEPS) {
        for (const tab of HELP_TABS[step]) {
          const { unmount } = renderWith(<HelpPanel view={{ step, tutorial: false }} actions={actions()} initialTab={tab.id} />, { locale });
          expect(ui(`help.${step}.${tab.id}`), `${locale} ${step} ${tab.id}`).not.toBeNull();
          unmount();
        }
      }
    }
  });

  it("titles the panel with the step and its place, and follows the step", () => {
    function Follow() {
      const [step, setStep] = useState<StepId>("measure");
      return (
        <>
          <button type="button" onClick={() => setStep("system")}>
            go
          </button>
          <HelpPanel view={{ step, tutorial: false }} actions={actions()} initialTab="mdat" />
        </>
      );
    }
    renderWith(<Follow />);
    expect(screen.getByText("도움말 · 측정·가져오기")).toBeTruthy();
    expect(ui("help.step")?.textContent).toBe("3 / 5");
    expect(ui("help.tab.mdat")?.getAttribute("aria-selected")).toBe("true");
    fireEvent.click(screen.getByText("go"));
    expect(screen.getByText("도움말 · 시스템")).toBeTruthy();
    expect(ui("help.tab.intro")?.getAttribute("aria-selected")).toBe("true");
  });

  it("switches tabs by click and arrow keys", () => {
    renderWith(<HelpPanel view={{ step: "export", tutorial: false }} actions={actions()} />);
    fireEvent.click(ui("help.tab.camilla")!);
    expect(ui("help.export.camilla")).not.toBeNull();
    fireEvent.keyDown(ui("help.tab.camilla")!, { key: "ArrowLeft" });
    expect(ui("help.export.real")).not.toBeNull();
    fireEvent.keyDown(ui("help.tab.real")!, { key: "Home" });
    expect(ui("help.export.steps")).not.toBeNull();
  });

  it("finds words, counts them per tab, moves with Enter and jumps to a tab that has them", () => {
    expect(countMatches("극성과 극성, POLARITY polarity", "polarity")).toBe(2);
    renderWith(<HelpPanel view={{ step: "system", tutorial: false }} actions={actions()} initialTab="checks" />);
    const input = ui("help.search") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "극성" } });
    // Not on 확인할 것: the count says where it is, Enter goes there.
    expect(screen.getByText(/다른 탭에 2곳/)).toBeTruthy();
    fireEvent.keyDown(input, { key: "Enter" });
    expect(ui("help.tab.steps")?.getAttribute("aria-selected")).toBe("true");
    expect(screen.getByText("1 / 2")).toBeTruthy();
    expect(document.querySelectorAll("mark.help-mark")).toHaveLength(2);
    fireEvent.keyDown(input, { key: "Enter" });
    expect(screen.getByText("2 / 2")).toBeTruthy();
    expect(document.querySelector("[data-current-match]")?.textContent).toBe("극성");
  });

  it("closes on Esc and the close button, and starts the tutorial from 처음이라면", () => {
    const act = actions();
    renderWith(<HelpPanel view={{ step: "system", tutorial: false }} actions={act} />);
    fireEvent.keyDown(ui("help.search")!, { key: "Escape" });
    fireEvent.click(ui("help.close")!);
    fireEvent.click(ui("help.intro.tutorial")!);
    expect(act.close).toHaveBeenCalledTimes(2);
    expect(act.startTutorial).toHaveBeenCalledTimes(1);
  });

  it("focuses the search on Ctrl+F", () => {
    renderWith(<HelpPanel view={{ step: "connect", tutorial: false }} actions={actions()} />);
    fireEvent.keyDown(window, { key: "f", code: "KeyF", ...(isMac() ? { metaKey: true } : { ctrlKey: true }) });
    expect(document.activeElement).toBe(ui("help.search"));
  });

  it("draws every id the reach table cites", () => {
    const seen = new Set<string>();
    for (const step of STEPS) {
      for (const tab of HELP_TABS[step]) {
        const { unmount } = renderWith(<HelpPanel view={{ step, tutorial: false }} actions={actions()} initialTab={tab.id} initialQuery="a" />);
        for (const node of document.querySelectorAll("[data-ui]")) seen.add(node.getAttribute("data-ui") ?? "");
        unmount();
      }
    }
    expect(REACH_IDS.filter((id) => !seen.has(id))).toEqual([]);
  });
});

it("gives focus back to where it was when the panel closes", () => {
  function Host() {
    const [open, setOpen] = useState(false);
    return <>
      <button type="button" onClick={() => setOpen(true)}>open help</button>
      {open && <HelpPanel view={{ step: "system", tutorial: false }} actions={{ close: () => setOpen(false), startTutorial: () => {} }} />}
    </>;
  }
  renderWith(<Host />);
  const opener = screen.getByText("open help");
  opener.focus();
  fireEvent.click(opener);
  expect(document.activeElement?.getAttribute("data-ui")).toBe("help");
  fireEvent.click(document.querySelector('[data-ui="help.close"]')!);
  expect(document.activeElement).toBe(opener);
});
