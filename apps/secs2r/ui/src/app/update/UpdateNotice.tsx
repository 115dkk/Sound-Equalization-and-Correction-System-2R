// The one notice after an update (ADR 0020, 흐름 5): a dialog that opens
// on its own the first time the new version runs. The title names the
// version, the body is that version's CHANGELOG section, and when versions
// were skipped one sentence links to the release page for theirs. 닫기
// (or Escape, or the close button) dismisses it for good; the service
// forgets it on `update_notice_dismiss` and the window hides it at once.
//
// It opens over 새 프로젝트 and the other dialogs of a start, but never over a
// confirm: while one is open (a measurement's 지우기, 관리 끄기) it waits.
// Opening beside a dialog that mounts in the same moment, it keeps focus,
// and when it closes focus goes back into the dialog left under it.
import { useEffect, useState } from "react";
import { useT } from "../../i18n/react";
import type { UpdateNotice } from "../../ipc/bindings/UpdateNotice";
import { Button } from "../../ui/Button";
import { Dialog, useConfirmOpen } from "../../ui/Dialog";
import { ReleaseNotes } from "./notes";
import { ReleaseLink } from "./parts";
import { useUpdateApi, type UpdateApi } from "./scope";

/** The notice for the frame's update scope (none without one). */
export function FrameUpdateNotice() {
  const api = useUpdateApi();
  return api ? <UpdateNoticeHost api={api} /> : null;
}

const ID = "update.notice";
const FOCUSABLE = 'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/** Keeps focus in the notice while it is the top dialog; on close, hands it to the dialog under it. */
function useKeepFocus(active: boolean) {
  useEffect(() => {
    if (!active) return;
    const keep = (event: FocusEvent) => {
      const panel = document.getElementById(ID);
      if (!panel || !(event.target instanceof Node) || panel.contains(event.target)) return;
      panel.querySelector<HTMLElement>("[data-autofocus]")?.focus();
    };
    document.addEventListener("focusin", keep);
    return () => {
      document.removeEventListener("focusin", keep);
      // After the dialog has put focus back where it was when it opened.
      window.setTimeout(() => {
        const current = document.activeElement;
        if (current && current !== document.body && current.isConnected) return;
        const under = [...document.querySelectorAll<HTMLElement>('[role="dialog"][aria-modal="true"]')].pop();
        (under?.querySelector<HTMLElement>("[data-autofocus]") ?? under?.querySelector<HTMLElement>(FOCUSABLE))?.focus();
      }, 0);
    };
  }, [active]);
}

export function UpdateNoticeDialog({
  notice,
  onDismiss,
  onOpenRelease,
  inline,
}: {
  notice: UpdateNotice;
  onDismiss: () => void;
  /** Opens the release page of `notice.to`, where the skipped versions are linked. */
  onOpenRelease: (version: string) => void;
  /** In place, without a backdrop (gallery). */
  inline?: boolean;
}) {
  const t = useT();
  useKeepFocus(!inline);
  return (
    <Dialog
      open
      above
      inline={inline}
      id={ID}
      onClose={onDismiss}
      title={t("update.notice.title", { version: notice.to })}
      footer={
        <Button variant="primary" data-ui="update.notice.dismiss" data-autofocus onClick={onDismiss}>
          {t("ui.close")}
        </Button>
      }
    >
      <div className="update-notice">
        {notice.skipped && (
          <p className="update-notice__skipped">
            {t("update.notice.skipped")}{" "}
            <ReleaseLink inline id="update.notice.release" onOpen={() => onOpenRelease(notice.to)}>
              {t("update.notice.release")}
            </ReleaseLink>
          </p>
        )}
        {notice.notes.trim() ? <ReleaseNotes markdown={notice.notes} id="update.notice.notes" /> : <p className="update-notice__empty">{t("update.notice.empty")}</p>}
      </div>
    </Dialog>
  );
}

const sameNotice = (a: UpdateNotice | null, b: UpdateNotice) => a !== null && a.from === b.from && a.to === b.to;

/** The notice while the service has one, except while a confirm is open; once dismissed here, it stays closed. */
export function UpdateNoticeHost({ api }: { api: UpdateApi }) {
  const confirmOpen = useConfirmOpen();
  const [closed, setClosed] = useState<UpdateNotice | null>(null);
  const notice = api.view.status?.notice ?? null;
  if (!notice || confirmOpen || sameNotice(closed, notice)) return null;
  return (
    <UpdateNoticeDialog
      notice={notice}
      onDismiss={() => {
        setClosed(notice);
        void api.actions.dismiss();
      }}
      onOpenRelease={(version) => void api.actions.openRelease(version)}
    />
  );
}
