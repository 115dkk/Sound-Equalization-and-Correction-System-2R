// The screen gallery (gallery.html, a second Vite entry that the app's main
// route never loads). Every component and the frame in each state, in the
// theme, language and accent the URL asks for:
//   gallery.html?theme=light|dark&lang=ko|en&accent=default|tutorial&skin=default|rack|matrix&only=frame.,ui.button
// Each specimen carries a stable data-gallery-id so CI can screenshot it alone.
import { useMemo, useState } from "react";
import { PrefsScope, SKINS, usePrefs, type Prefs, type SkinChoice } from "../app/prefs";
import "../ui";
import { Segmented } from "../ui/controls";
import { frameSpecimens, trayAndMenuSpecimens } from "./frameSpecimens";
import { kitSpecimens } from "./kitSpecimens";
import { localApiSpecimens } from "./localApiSpecimens";
import { hostShellSpecimens } from "../host/specimens";
import { mapSpecimens, plotSpecimens } from "./plotSpecimens";
import { screenSections } from "../screens/specimens";
import { Specimen, type SectionDef } from "./Specimen";
import { appSpecimens } from "./appSpecimen";
import { tutorialLiveSpecimens } from "./tutorialLive";

const SECTIONS: SectionDef[] = [
  { id: "app", title: "Application", specimens: [...appSpecimens, ...tutorialLiveSpecimens] },
  { id: "frame", title: "App frame", specimens: frameSpecimens },
  { id: "jobs", title: "Job tray and frame menus", specimens: [...trayAndMenuSpecimens, ...localApiSpecimens] },
  { id: "kit", title: "Component kit", specimens: kitSpecimens },
  { id: "plot", title: "Graphs", specimens: plotSpecimens },
  { id: "maps", title: "Maps", specimens: mapSpecimens },
  ...screenSections,
  { id: "host", title: "Host shell (host.html)", specimens: hostShellSpecimens },
];

function readParams(): { prefs: Prefs; only: string[] } {
  const params = new URLSearchParams(window.location.search);
  return {
    prefs: {
      theme: params.get("theme") === "dark" ? "dark" : "light",
      locale: params.get("lang") === "en" ? "en" : "ko",
      accent: params.get("accent") === "tutorial" ? "tutorial" : "default",
      skin: (SKINS as readonly string[]).includes(params.get("skin") ?? "") ? (params.get("skin") as SkinChoice) : "default",
    },
    only: (params.get("only") ?? "").split(",").map((s) => s.trim()).filter(Boolean),
  };
}

function writeParam(name: string, value: string) {
  const url = new URL(window.location.href);
  url.searchParams.set(name, value);
  window.history.replaceState(null, "", url);
}

function Toolbar() {
  const prefs = usePrefs();
  return (
    <div className="gal-toolbar" role="toolbar" aria-label="Gallery options">
      <strong className="gal-toolbar__title">SECS 2R gallery</strong>
      <Segmented
        id="gallery.theme"
        label="Theme"
        size="sm"
        value={prefs.theme === "dark" ? "dark" : "light"}
        options={[
          { value: "light", label: "Light" },
          { value: "dark", label: "Dark" },
        ]}
        onChange={(theme) => {
          writeParam("theme", theme);
          prefs.set({ theme });
        }}
      />
      <Segmented
        id="gallery.lang"
        label="Language"
        size="sm"
        value={prefs.locale}
        options={[
          { value: "ko", label: "한국어" },
          { value: "en", label: "English" },
        ]}
        onChange={(locale) => {
          writeParam("lang", locale);
          prefs.set({ locale });
        }}
      />
      <Segmented
        id="gallery.accent"
        label="Accent"
        size="sm"
        value={prefs.accent}
        options={[
          { value: "default", label: "Default" },
          { value: "tutorial", label: "Tutorial" },
        ]}
        onChange={(accent) => {
          writeParam("accent", accent);
          prefs.set({ accent });
        }}
      />
      <Segmented<SkinChoice>
        id="gallery.skin"
        label="Skin"
        size="sm"
        value={prefs.skin ?? "default"}
        options={[
          { value: "default", label: "Default" },
          { value: "rack", label: "Rack" },
          { value: "matrix", label: "Matrix" },
        ]}
        onChange={(skin) => {
          writeParam("skin", skin);
          prefs.set({ skin });
        }}
      />
    </div>
  );
}

export function Gallery() {
  const [{ prefs, only }] = useState(readParams);
  const sections = useMemo(
    () =>
      SECTIONS.map((section) => ({
        ...section,
        specimens: section.specimens.filter((s) => only.length === 0 || only.some((prefix) => s.id.startsWith(prefix))),
      })).filter((section) => section.specimens.length > 0),
    [only],
  );
  return (
    <PrefsScope initial={prefs} root>
      <div className="gal-page">
        <Toolbar />
        <nav className="gal-index" aria-label="Sections">
          {sections.map((section) => (
            <a key={section.id} href={`#gal-section-${section.id}`}>
              {section.title}
            </a>
          ))}
        </nav>
        {sections.map((section) => (
          <section key={section.id} className="gal-section" id={`gal-section-${section.id}`} aria-labelledby={`gal-section-${section.id}-title`}>
            <h2 className="gal-section__title" id={`gal-section-${section.id}-title`}>
              {section.title}
            </h2>
            <div className="gal-section__items">
              {section.specimens.map((def) => (
                <Specimen key={def.id} def={def} />
              ))}
            </div>
          </section>
        ))}
      </div>
    </PrefsScope>
  );
}
