// The help text of each step as catalog keys (help.<step>.<tab>.*): a
// rewrite of upstream src/secs/wizard/tutorial_text.py for the five steps.
// The tables are data; the words live in the catalogs. Key names follow the
// tab and the item's place, so adding an item means a new key in both
// catalogs (ko is the source) and a line here.
import type { StepId } from "../../app/model";
import type { ChannelRow, HelpTab } from "./types";

export const HELP_TABS: Record<StepId, readonly HelpTab[]> = {
  system: [
    {
      id: "intro",
      blocks: [
        {
          kind: "paragraphs",
          items: [
            "help.system.intro.body.1",
            "help.system.intro.body.2",
            "help.system.intro.body.3",
            "help.system.intro.body.4",
          ],
        },
        { kind: "tutorial" },
        { kind: "heading", text: "help.system.intro.route_head" },
        {
          kind: "numbered",
          items: [
            "help.system.intro.route.1",
            "help.system.intro.route.2",
            "help.system.intro.route.3",
            "help.system.intro.route.4",
            "help.system.intro.route.5",
            "help.system.intro.route.6",
          ],
        },
        {
          kind: "paragraphs",
          items: [
            "help.system.intro.outro.1",
          ],
        },
      ],
    },
    {
      id: "steps",
      blocks: [
        { kind: "lead", text: "help.system.steps.goal" },
        {
          kind: "steps",
          items: [
            { do: "help.system.steps.1.do", see: "help.system.steps.1.see" },
            { do: "help.system.steps.2.do", see: "help.system.steps.2.see" },
            { do: "help.system.steps.3.do", see: "help.system.steps.3.see" },
            { do: "help.system.steps.4.do", see: "help.system.steps.4.see" },
            { do: "help.system.steps.5.do", see: "help.system.steps.5.see" },
            { do: "help.system.steps.6.do", see: "help.system.steps.6.see" },
            { do: "help.system.steps.7.do", see: "help.system.steps.7.see" },
            { do: "help.system.steps.8.do", see: "help.system.steps.8.see" },
            { do: "help.system.steps.9.do", see: "help.system.steps.9.see" },
            { do: "help.system.steps.10.do", see: "help.system.steps.10.see" },
            { do: "help.system.steps.11.do", see: "help.system.steps.11.see" },
          ],
        },
      ],
    },
    {
      id: "checks",
      blocks: [
        {
          kind: "bullets",
          items: [
            "help.system.checks.1",
            "help.system.checks.2",
            "help.system.checks.3",
            "help.system.checks.4",
            "help.system.checks.5",
            "help.system.checks.6",
          ],
        },
      ],
    },
    {
      id: "trouble",
      blocks: [
        {
          kind: "pairs",
          items: [
            { term: "help.system.trouble.1.symptom", text: "help.system.trouble.1.fix" },
            { term: "help.system.trouble.2.symptom", text: "help.system.trouble.2.fix" },
            { term: "help.system.trouble.3.symptom", text: "help.system.trouble.3.fix" },
            { term: "help.system.trouble.4.symptom", text: "help.system.trouble.4.fix" },
            { term: "help.system.trouble.5.symptom", text: "help.system.trouble.5.fix" },
          ],
        },
      ],
    },
    {
      id: "real",
      blocks: [
        {
          kind: "bullets",
          items: [
            "help.system.real.1",
            "help.system.real.2",
            "help.system.real.3",
            "help.system.real.4",
            "help.system.real.5",
            "help.system.real.6",
          ],
        },
      ],
    },
  ],
  connect: [
    {
      id: "steps",
      blocks: [
        { kind: "lead", text: "help.connect.steps.goal" },
        {
          kind: "steps",
          items: [
            { do: "help.connect.steps.1.do", see: "help.connect.steps.1.see" },
            { do: "help.connect.steps.2.do", see: "help.connect.steps.2.see" },
            { do: "help.connect.steps.3.do", see: "help.connect.steps.3.see" },
            { do: "help.connect.steps.4.do", see: "help.connect.steps.4.see" },
            { do: "help.connect.steps.5.do", see: "help.connect.steps.5.see" },
            { do: "help.connect.steps.6.do", see: "help.connect.steps.6.see" },
            { do: "help.connect.steps.7.do", see: "help.connect.steps.7.see" },
            { do: "help.connect.steps.8.do", see: "help.connect.steps.8.see" },
            { do: "help.connect.steps.9.do", see: "help.connect.steps.9.see" },
          ],
        },
      ],
    },
    {
      id: "checks",
      blocks: [
        {
          kind: "bullets",
          items: [
            "help.connect.checks.1",
            "help.connect.checks.2",
            "help.connect.checks.3",
            "help.connect.checks.4",
            "help.connect.checks.5",
            "help.connect.checks.6",
          ],
        },
      ],
    },
    {
      id: "trouble",
      blocks: [
        {
          kind: "pairs",
          items: [
            { term: "help.connect.trouble.1.symptom", text: "help.connect.trouble.1.fix" },
            { term: "help.connect.trouble.2.symptom", text: "help.connect.trouble.2.fix" },
            { term: "help.connect.trouble.3.symptom", text: "help.connect.trouble.3.fix" },
            { term: "help.connect.trouble.4.symptom", text: "help.connect.trouble.4.fix" },
            { term: "help.connect.trouble.5.symptom", text: "help.connect.trouble.5.fix" },
            { term: "help.connect.trouble.6.symptom", text: "help.connect.trouble.6.fix" },
            { term: "help.connect.trouble.7.symptom", text: "help.connect.trouble.7.fix" },
            { term: "help.connect.trouble.8.symptom", text: "help.connect.trouble.8.fix" },
          ],
        },
      ],
    },
    {
      id: "real",
      blocks: [
        {
          kind: "bullets",
          items: [
            "help.connect.real.1",
            "help.connect.real.2",
            "help.connect.real.3",
            "help.connect.real.4",
            "help.connect.real.5",
            "help.connect.real.6",
          ],
        },
      ],
    },
  ],
  measure: [
    {
      id: "steps",
      blocks: [
        { kind: "lead", text: "help.measure.steps.goal" },
        {
          kind: "steps",
          items: [
            { do: "help.measure.steps.1.do", see: "help.measure.steps.1.see" },
            { do: "help.measure.steps.2.do", see: "help.measure.steps.2.see" },
            { do: "help.measure.steps.3.do", see: "help.measure.steps.3.see" },
            { do: "help.measure.steps.4.do", see: "help.measure.steps.4.see" },
            { do: "help.measure.steps.5.do", see: "help.measure.steps.5.see" },
            { do: "help.measure.steps.6.do", see: "help.measure.steps.6.see" },
            { do: "help.measure.steps.7.do", see: "help.measure.steps.7.see" },
          ],
        },
      ],
    },
    {
      id: "checks",
      blocks: [
        {
          kind: "bullets",
          items: [
            "help.measure.checks.1",
            "help.measure.checks.2",
            "help.measure.checks.3",
            "help.measure.checks.4",
            "help.measure.checks.5",
            "help.measure.checks.6",
          ],
        },
      ],
    },
    {
      id: "trouble",
      blocks: [
        {
          kind: "pairs",
          items: [
            { term: "help.measure.trouble.1.symptom", text: "help.measure.trouble.1.fix" },
            { term: "help.measure.trouble.2.symptom", text: "help.measure.trouble.2.fix" },
            { term: "help.measure.trouble.3.symptom", text: "help.measure.trouble.3.fix" },
            { term: "help.measure.trouble.4.symptom", text: "help.measure.trouble.4.fix" },
          ],
        },
      ],
    },
    {
      id: "real",
      blocks: [
        {
          kind: "bullets",
          items: [
            "help.measure.real.1",
            "help.measure.real.2",
            "help.measure.real.3",
            "help.measure.real.4",
          ],
        },
      ],
    },
    {
      id: "mdat",
      blocks: [
        {
          kind: "paragraphs",
          items: [
            "help.measure.mdat.start.1",
            "help.measure.mdat.start.2",
          ],
        },
        { kind: "heading", text: "help.measure.mdat.s1.head" },
        {
          kind: "bullets",
          items: [
            "help.measure.mdat.s1.1",
            "help.measure.mdat.s1.2",
            "help.measure.mdat.s1.3",
          ],
        },
        { kind: "heading", text: "help.measure.mdat.s2.head" },
        {
          kind: "bullets",
          items: [
            "help.measure.mdat.s2.1",
            "help.measure.mdat.s2.2",
            "help.measure.mdat.s2.3",
            "help.measure.mdat.s2.4",
            "help.measure.mdat.s2.5",
          ],
        },
        { kind: "heading", text: "help.measure.mdat.s3.head" },
        {
          kind: "bullets",
          items: [
            "help.measure.mdat.s3.1",
            "help.measure.mdat.s3.2",
            "help.measure.mdat.s3.3",
          ],
        },
        { kind: "heading", text: "help.measure.mdat.s4.head" },
        { kind: "channels" },
        {
          kind: "bullets",
          items: [
            "help.measure.mdat.s4.1",
            "help.measure.mdat.s4.2",
            "help.measure.mdat.s4.3",
          ],
        },
        { kind: "heading", text: "help.measure.mdat.s5.head" },
        {
          kind: "bullets",
          items: [
            "help.measure.mdat.s5.1",
            "help.measure.mdat.s5.2",
            "help.measure.mdat.s5.3",
            "help.measure.mdat.s5.4",
          ],
        },
        { kind: "heading", text: "help.measure.mdat.s6.head" },
        {
          kind: "bullets",
          items: [
            "help.measure.mdat.s6.1",
            "help.measure.mdat.s6.2",
          ],
        },
      ],
    },
  ],
  calibrate: [
    {
      id: "steps",
      blocks: [
        { kind: "lead", text: "help.calibrate.steps.goal" },
        {
          kind: "steps",
          items: [
            { do: "help.calibrate.steps.1.do", see: "help.calibrate.steps.1.see" },
            { do: "help.calibrate.steps.2.do", see: "help.calibrate.steps.2.see" },
            { do: "help.calibrate.steps.3.do", see: "help.calibrate.steps.3.see" },
            { do: "help.calibrate.steps.4.do", see: "help.calibrate.steps.4.see" },
            { do: "help.calibrate.steps.5.do", see: "help.calibrate.steps.5.see" },
            { do: "help.calibrate.steps.6.do", see: "help.calibrate.steps.6.see" },
            { do: "help.calibrate.steps.7.do", see: "help.calibrate.steps.7.see" },
            { do: "help.calibrate.steps.8.do", see: "help.calibrate.steps.8.see" },
            { do: "help.calibrate.steps.9.do", see: "help.calibrate.steps.9.see" },
          ],
        },
      ],
    },
    {
      id: "checks",
      blocks: [
        {
          kind: "bullets",
          items: [
            "help.calibrate.checks.1",
            "help.calibrate.checks.2",
            "help.calibrate.checks.3",
            "help.calibrate.checks.4",
            "help.calibrate.checks.5",
            "help.calibrate.checks.6",
          ],
        },
      ],
    },
    {
      id: "trouble",
      blocks: [
        {
          kind: "pairs",
          items: [
            { term: "help.calibrate.trouble.1.symptom", text: "help.calibrate.trouble.1.fix" },
            { term: "help.calibrate.trouble.2.symptom", text: "help.calibrate.trouble.2.fix" },
            { term: "help.calibrate.trouble.3.symptom", text: "help.calibrate.trouble.3.fix" },
            { term: "help.calibrate.trouble.4.symptom", text: "help.calibrate.trouble.4.fix" },
          ],
        },
      ],
    },
    {
      id: "real",
      blocks: [
        {
          kind: "bullets",
          items: [
            "help.calibrate.real.1",
            "help.calibrate.real.2",
            "help.calibrate.real.3",
          ],
        },
      ],
    },
  ],
  export: [
    {
      id: "steps",
      blocks: [
        { kind: "lead", text: "help.export.steps.goal" },
        {
          kind: "steps",
          items: [
            { do: "help.export.steps.1.do", see: "help.export.steps.1.see" },
            { do: "help.export.steps.2.do", see: "help.export.steps.2.see" },
            { do: "help.export.steps.3.do", see: "help.export.steps.3.see" },
            { do: "help.export.steps.4.do", see: "help.export.steps.4.see" },
            { do: "help.export.steps.5.do", see: "help.export.steps.5.see" },
            { do: "help.export.steps.6.do", see: "help.export.steps.6.see" },
          ],
        },
      ],
    },
    {
      id: "checks",
      blocks: [
        {
          kind: "bullets",
          items: [
            "help.export.checks.1",
            "help.export.checks.2",
            "help.export.checks.3",
            "help.export.checks.4",
          ],
        },
      ],
    },
    {
      id: "trouble",
      blocks: [
        {
          kind: "pairs",
          items: [
            { term: "help.export.trouble.1.symptom", text: "help.export.trouble.1.fix" },
            { term: "help.export.trouble.2.symptom", text: "help.export.trouble.2.fix" },
            { term: "help.export.trouble.3.symptom", text: "help.export.trouble.3.fix" },
          ],
        },
      ],
    },
    {
      id: "real",
      blocks: [
        {
          kind: "bullets",
          items: [
            "help.export.real.1",
            "help.export.real.2",
            "help.export.real.3",
          ],
        },
      ],
    },
    {
      id: "camilla",
      blocks: [
        {
          kind: "paragraphs",
          items: [
            "help.export.camilla.start.1",
            "help.export.camilla.start.2",
          ],
        },
        { kind: "heading", text: "help.export.camilla.s1.head" },
        {
          kind: "bullets",
          items: [
            "help.export.camilla.s1.1",
            "help.export.camilla.s1.2",
            "help.export.camilla.s1.3",
            "help.export.camilla.s1.4",
          ],
        },
        { kind: "heading", text: "help.export.camilla.s2.head" },
        {
          kind: "bullets",
          items: [
            "help.export.camilla.s2.1",
            "help.export.camilla.s2.2",
            "help.export.camilla.s2.3",
            "help.export.camilla.s2.4",
            "help.export.camilla.s2.5",
          ],
        },
        { kind: "heading", text: "help.export.camilla.s3.head" },
        {
          kind: "bullets",
          items: [
            "help.export.camilla.s3.1",
            "help.export.camilla.s3.2",
            "help.export.camilla.s3.3",
          ],
        },
        { kind: "heading", text: "help.export.camilla.s4.head" },
        {
          kind: "bullets",
          items: [
            "help.export.camilla.s4.1",
            "help.export.camilla.s4.2",
            "help.export.camilla.s4.3",
            "help.export.camilla.s4.4",
          ],
        },
        { kind: "heading", text: "help.export.camilla.s5.head" },
        {
          kind: "bullets",
          items: [
            "help.export.camilla.s5.1",
            "help.export.camilla.s5.2",
            "help.export.camilla.s5.3",
            "help.export.camilla.s5.4",
          ],
        },
        {
          kind: "code",
          items: [
            "help.export.camilla.s5.code.1",
            "help.export.camilla.s5.code.2",
            "help.export.camilla.s5.code.3",
            "help.export.camilla.s5.code.4",
            "help.export.camilla.s5.code.5",
          ],
        },
        { kind: "heading", text: "help.export.camilla.s6.head" },
        {
          kind: "bullets",
          items: [
            "help.export.camilla.s6.1",
            "help.export.camilla.s6.2",
            "help.export.camilla.s6.3",
            "help.export.camilla.s6.4",
            "help.export.camilla.s6.5",
            "help.export.camilla.s6.6",
            "help.export.camilla.s6.7",
          ],
        },
        { kind: "heading", text: "help.export.camilla.s7.head" },
        {
          kind: "bullets",
          items: [
            "help.export.camilla.s7.1",
            "help.export.camilla.s7.2",
            "help.export.camilla.s7.3",
            "help.export.camilla.s7.4",
            "help.export.camilla.s7.5",
            "help.export.camilla.s7.6",
            "help.export.camilla.s7.7",
            "help.export.camilla.s7.8",
            "help.export.camilla.s7.9",
          ],
        },
        { kind: "heading", text: "help.export.camilla.s8.head" },
        {
          kind: "bullets",
          items: [
            "help.export.camilla.s8.1",
            "help.export.camilla.s8.2",
            "help.export.camilla.s8.3",
          ],
        },
        { kind: "heading", text: "help.export.camilla.s9.head" },
        {
          kind: "bullets",
          items: [
            "help.export.camilla.s9.1",
            "help.export.camilla.s9.2",
            "help.export.camilla.s9.3",
            "help.export.camilla.s9.4",
          ],
        },
        { kind: "heading", text: "help.export.camilla.s10.head" },
        {
          kind: "bullets",
          items: [
            "help.export.camilla.s10.1",
            "help.export.camilla.s10.2",
            "help.export.camilla.s10.3",
            "help.export.camilla.s10.4",
          ],
        },
        { kind: "heading", text: "help.export.camilla.s11.head" },
        {
          kind: "bullets",
          items: [
            "help.export.camilla.s11.1",
            "help.export.camilla.s11.2",
            "help.export.camilla.s11.3",
            "help.export.camilla.s11.4",
            "help.export.camilla.s11.5",
          ],
        },
        { kind: "heading", text: "help.export.camilla.s12.head" },
        {
          kind: "bullets",
          items: [
            "help.export.camilla.s12.1",
            "help.export.camilla.s12.2",
          ],
        },
        { kind: "heading", text: "help.export.camilla.s13.head" },
        {
          kind: "bullets",
          items: [
            "help.export.camilla.s13.1",
            "help.export.camilla.s13.2",
            "help.export.camilla.s13.3",
            "help.export.camilla.s13.4",
            "help.export.camilla.s13.5",
            "help.export.camilla.s13.6",
            "help.export.camilla.s13.7",
            "help.export.camilla.s13.8",
          ],
        },
      ],
    },
  ],
};

/** Channel names for mdat titles (upstream tools/make_mdat_help_shots.py CHANNEL_ROWS). Names are codes and stay untranslated. */
export const MDAT_CHANNEL_ROWS: readonly ChannelRow[] = [
  { group: "help.measure.mdat.group.front", use: "L · R · C", also: "FL · FR · FC · CEN" },
  { group: "help.measure.mdat.group.side", use: "Ls · Rs", also: "SL · SR" },
  { group: "help.measure.mdat.group.rear", use: "Lrs · Rrs", also: "SBL · SBR · BL · BR" },
  { group: "help.measure.mdat.group.wide", use: "Lw · Rw", also: "FWL · FWR" },
  { group: "help.measure.mdat.group.top", use: "Ltf · Rtf · Ltm · Rtm · Ltr · Rtr", also: "TFL · TFR · TML · TMR · TRL · TRR" },
  { group: "help.measure.mdat.group.height", use: "Lfh · Rfh · Lrh · Rrh", also: "FHL · FHR · RHL · RHR" },
  { group: "help.measure.mdat.group.subs", use: "SW1 · SW2 · … · SW8", also: "SUB · LFE · Sub2 · LFE2 …" },
];
