import { useId, type ReactNode } from "react";
import { useT } from "../i18n/react";
import { Icon } from "./Icon";
import { Tooltip } from "./Tooltip";

export const helpContentId = (id: string) => `${id}.help-content`;
/** A separate button leaves the field label and its control's action intact. */
export function HelpTip({ content, labelledBy, label, id, side = "top" }: {
  content: ReactNode; labelledBy?: string; label?: string; id?: string; side?: "top" | "bottom";
}) {
  const t = useT();
  const generated = useId();
  const key = id ?? generated;
  const suffix = `${key}-label`;
  return <Tooltip id={`${key}-content`} content={content} toggle side={side}>
    <button type="button" className="ui-help" data-ui={id} data-help-trigger="true"
      aria-labelledby={labelledBy ? `${labelledBy} ${suffix}` : suffix}>
      <Icon name="info" size={16} />
      <span id={suffix} className="visually-hidden">{label ? `${label} ${t("ui.help")}` : t("ui.help")}</span>
    </button>
  </Tooltip>;
}
