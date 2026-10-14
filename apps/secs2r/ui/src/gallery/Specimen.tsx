// One gallery specimen: a stable `data-gallery-id` for CI to screenshot it
// on its own, a developer title, and the component drawn on the theme's
// background. Gallery chrome is a developer tool, not app text, so its
// titles are plain English and not catalog keys.
import type { ReactNode } from "react";

export type SpecimenDef = {
  id: string;
  title: string;
  /** Fixed stage width in px (frames are 1280 × 800). */
  width?: number;
  height?: number;
  render: () => ReactNode;
};

export type SectionDef = { id: string; title: string; specimens: SpecimenDef[] };

export function Specimen({ def }: { def: SpecimenDef }) {
  return (
    <section className="gal-specimen" data-gallery-id={def.id} aria-labelledby={`gal-${def.id}`}>
      <header className="gal-specimen__head">
        <h3 className="gal-specimen__title" id={`gal-${def.id}`}>
          {def.title}
        </h3>
        <code className="gal-specimen__id">{def.id}</code>
      </header>
      <div
        className="gal-specimen__stage"
        style={def.width || def.height ? { width: def.width, height: def.height } : undefined}
      >
        {def.render()}
      </div>
    </section>
  );
}
