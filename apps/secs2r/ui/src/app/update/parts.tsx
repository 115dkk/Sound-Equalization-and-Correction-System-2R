// Small pieces the update badge, the notice and the settings section share:
// the state glyph (the rail status's dot or spinner) and the control that
// opens a release's page.
import type { ReactNode } from "react";
import { Button } from "../../ui/Button";
import { Spinner } from "../../ui/Icon";
import type { UpdateTone } from "./model";
import "./update.css";

/** The state's glyph in a box one text line tall, so wrapped words keep it on the first line. */
export function UpdateGlyph({ tone }: { tone: UpdateTone }) {
  return (
    <span className="update-glyph" aria-hidden="true">
      {tone === "busy" ? <Spinner size={12} className="status-dot status-dot--busy" /> : <span className={tone === "off" ? "status-dot" : `status-dot status-dot--${tone}`} />}
    </span>
  );
}

/**
 * Opens a release's page in the system browser through the service
 * (`update_open_release`), never by navigating the app's own window.
 * `inline` draws it as a link inside a sentence; otherwise it is a small
 * secondary button on a line of its own.
 */
export function ReleaseLink({ id, onOpen, inline = false, children }: { id: string; onOpen: () => void; inline?: boolean; children: ReactNode }) {
  if (!inline) {
    return (
      <Button size="sm" data-ui={id} onClick={onOpen}>
        {children}
      </Button>
    );
  }
  return (
    <button type="button" className="update-link" data-ui={id} onClick={onOpen}>
      {children}
    </button>
  );
}
