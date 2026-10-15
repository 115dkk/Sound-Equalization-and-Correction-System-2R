// The glossary (upstream tutorial_quests.py:462-511): each term's name and
// one-line meaning, shown under the first quest card that names it.
import type { MessageKey } from "../../i18n";
import type { GlossaryTerm } from "./types";

export const GLOSSARY: Record<GlossaryTerm, { term: MessageKey; text: MessageKey }> = {
  rew: { term: "tutorial.glossary.rew.term", text: "tutorial.glossary.rew" },
  api: { term: "tutorial.glossary.api.term", text: "tutorial.glossary.api" },
  calibration: { term: "tutorial.glossary.calibration.term", text: "tutorial.glossary.calibration" },
  sweep: { term: "tutorial.glossary.sweep.term", text: "tutorial.glossary.sweep" },
  polarity: { term: "tutorial.glossary.polarity.term", text: "tutorial.glossary.polarity" },
  dsp: { term: "tutorial.glossary.dsp.term", text: "tutorial.glossary.dsp" },
  fir: { term: "tutorial.glossary.fir.term", text: "tutorial.glossary.fir" },
  iir: { term: "tutorial.glossary.iir.term", text: "tutorial.glossary.iir" },
  dbfs: { term: "tutorial.glossary.dbfs.term", text: "tutorial.glossary.dbfs" },
  timing: { term: "tutorial.glossary.timing.term", text: "tutorial.glossary.timing" },
  mimo: { term: "tutorial.glossary.mimo.term", text: "tutorial.glossary.mimo" },
  mdat: { term: "tutorial.glossary.mdat.term", text: "tutorial.glossary.mdat" },
  rc: { term: "tutorial.glossary.rc.term", text: "tutorial.glossary.rc" },
  spl: { term: "tutorial.glossary.spl.term", text: "tutorial.glossary.spl" },
  target: { term: "tutorial.glossary.target.term", text: "tutorial.glossary.target" },
  gd: { term: "tutorial.glossary.gd.term", text: "tutorial.glossary.gd" },
  xo: { term: "tutorial.glossary.xo.term", text: "tutorial.glossary.xo" },
  peq: { term: "tutorial.glossary.peq.term", text: "tutorial.glossary.peq" },
  boost: { term: "tutorial.glossary.boost.term", text: "tutorial.glossary.boost" },
  preset: { term: "tutorial.glossary.preset.term", text: "tutorial.glossary.preset" },
  lp_hp: { term: "tutorial.glossary.lp_hp.term", text: "tutorial.glossary.lp_hp" },
  gain: { term: "tutorial.glossary.gain.term", text: "tutorial.glossary.gain" },
  delay: { term: "tutorial.glossary.delay.term", text: "tutorial.glossary.delay" },
  allpass: { term: "tutorial.glossary.allpass.term", text: "tutorial.glossary.allpass" },
  dba: { term: "tutorial.glossary.dba.term", text: "tutorial.glossary.dba" },
};
