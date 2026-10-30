// Pieces the export screen and the host shell share (ADR 0018, 관리).
import { useT } from "../i18n/react";
import { ConfirmDialog } from "../ui";
import { stopBody, type HostState } from "./model";

/**
 * 관리 끄기's confirm: the action named in the title and on its button, and
 * one sentence on what stops for what the service manages now. The service
 * lets go of both targets at once (`HostRequest::StopManaging`).
 */
export function StopManagingConfirm({
  id,
  open,
  host,
  onClose,
  onConfirm,
  inline,
}: {
  id: string;
  open: boolean;
  host: HostState | null;
  onClose: () => void;
  onConfirm: () => void;
  inline?: boolean;
}) {
  const t = useT();
  return (
    <ConfirmDialog id={id} open={open} inline={inline} title={t("host.stop.title")} body={t(stopBody(host))} confirmLabel={t("host.stop")} onClose={onClose} onConfirm={onConfirm} />
  );
}
