// Updates in the settings popover, under the local API: the policy (자동
// 설치, 알림만, 확인 안 함, with what each does behind the help button),
// the updater's state with 지금 확인 beside it, the state's own action
// (받아서 설치 when a version is available, the release page for a portable
// copy, the reason of a failure behind 자세히), and the version and the
// last check on one caption line.
import { useRef, useState } from "react";
import { useLocale, useT } from "../../i18n/react";
import type { UpdatePhase } from "../../ipc/bindings/UpdatePhase";
import type { UpdatePolicy } from "../../ipc/bindings/UpdatePolicy";
import { Button } from "../../ui/Button";
import { Segmented } from "../../ui/controls";
import { HelpTip } from "../../ui/HelpTip";
import { checkedText, phaseText, phaseTone } from "./model";
import { ReleaseLink, UpdateGlyph } from "./parts";
import type { UpdateApi } from "./scope";

const POLICIES = ["auto", "notify", "off"] as const satisfies readonly UpdatePolicy[];

const POLICY_LABEL = {
  auto: "settings.updates.auto",
  notify: "settings.updates.notify",
  off: "settings.updates.off",
} as const satisfies Record<UpdatePolicy, string>;

/** 지금 확인 is off while the updater is checking, downloading or installing (the state line says which). */
const CHECK_BUSY: ReadonlySet<UpdatePhase["kind"]> = new Set(["checking", "downloading", "installing"]);

export function UpdateSettings({ api }: { api: UpdateApi }) {
  const t = useT();
  const locale = useLocale();
  const { view, actions } = api;
  const status = view.status;
  // The choice shows at once; it falls back to the saved one if saving fails.
  const [choosing, setChoosing] = useState<UpdatePolicy | null>(null);
  // Only the latest choice's save puts the saved policy back on screen.
  const latest = useRef(0);
  if (!status) {
    return (
      <section className="update-settings" aria-label={t("settings.updates")}>
        <p className="update-settings__state">
          <UpdateGlyph tone="busy" />
          <span>{t("settings.updates.loading")}</span>
        </p>
      </section>
    );
  }
  const phase = status.phase;
  const choose = (policy: UpdatePolicy) => {
    if (policy === status.policy && choosing === null) return;
    const ticket = ++latest.current;
    setChoosing(policy);
    void actions.setPolicy(policy).finally(() => {
      if (ticket === latest.current) setChoosing(null);
    });
  };
  const meta = status.checked_at
    ? t("settings.updates.meta", { version: status.current, time: checkedText(status.checked_at, locale) })
    : t("settings.updates.meta.never", { version: status.current });

  return (
    <section className="update-settings" aria-label={t("settings.updates")}>
      {/* The help goes beside the label: after the three choices it would
          take a line of its own in English. */}
      <div className="ui-segmented-field">
        <div className="ui-heading-help">
          <span className="ui-field__label" id="settings.updates.label">
            {t("settings.updates")}
          </span>
          <HelpTip id="settings.updates.help" labelledBy="settings.updates.label" content={t("settings.updates.help")} />
        </div>
        <Segmented<UpdatePolicy>
          id="settings.updates"
          label={t("settings.updates")}
          hideLabel
          value={choosing ?? status.policy}
          options={POLICIES.map((policy) => ({ value: policy, label: t(POLICY_LABEL[policy]) }))}
          onChange={choose}
        />
      </div>
      <div className="update-settings__line">
        <p className="update-settings__state" data-ui="settings.updates.state" data-state={phase.kind} role="status">
          <UpdateGlyph tone={phaseTone(phase)} />
          <span>{phaseText(t, phase)}</span>
        </p>
        <Button variant="quiet" size="sm" data-ui="update.check" disabled={CHECK_BUSY.has(phase.kind)} onClick={() => void actions.check()}>
          {t("settings.updates.check")}
        </Button>
      </div>
      {phase.kind === "available" && (
        <Button size="sm" className="update-settings__action" data-ui="update.download" onClick={() => void actions.download()}>
          {t("settings.updates.download")}
        </Button>
      )}
      {phase.kind === "portable" && (
        <div className="update-settings__note">
          <p>{t("settings.updates.portable")}</p>
          <ReleaseLink id="settings.updates.release" onOpen={() => void actions.openRelease(phase.version)}>
            {t("update.release_page")}
          </ReleaseLink>
        </div>
      )}
      {phase.kind === "failed" && (
        <div className="update-settings__note">
          <p>{t("settings.updates.failed.next")}</p>
          <details className="ui-details" data-ui="settings.updates.reason">
            <summary>{t("ui.details")}</summary>
            <pre className="ui-details__raw">{phase.reason}</pre>
          </details>
        </div>
      )}
      <p className="update-settings__meta tabular" data-ui="settings.updates.meta">
        {meta}
      </p>
    </section>
  );
}
