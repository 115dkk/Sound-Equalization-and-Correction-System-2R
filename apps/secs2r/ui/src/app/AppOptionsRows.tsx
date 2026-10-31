// Two sections of the settings popover (menus.tsx). 낡은 결과 자동 갱신
// (ADR 0029): a switch, with what it recomputes, when and with how much of
// the CPU behind its help button; it sits after the display settings, the
// first setting about what the app does rather than how it looks. The log
// folder (ADR 0027): 로그 폴더 열기 and the sentence that says what the log
// is for, last in the popover; not drawn when the app keeps no log.
import { useT } from "../i18n/react";
import { Button } from "../ui/Button";
import { Toggle } from "../ui/controls";
import type { AppOptions } from "./appOptions";

export function AutoRefreshRow({ options, helpOpen }: { options: AppOptions; /** Gallery: the help shown open. */ helpOpen?: boolean | undefined }) {
  const t = useT();
  const { view, actions } = options;
  return (
    <div className="app-options-row" data-ui="app.auto_refresh.row">
      <Toggle
        id="app.auto_refresh"
        label={t("app.auto_refresh")}
        help={t("app.auto_refresh.help")}
        helpOpen={helpOpen}
        checked={view.autoRefresh ?? false}
        disabled={view.autoRefresh === null}
        // The switch shows the saved value; a second press while the first is saved is dropped.
        onChange={(on) => {
          if (!view.saving) actions.setAutoRefresh(on);
        }}
      />
    </div>
  );
}

export function AppLogRow({ options }: { options: AppOptions }) {
  const t = useT();
  const { view, actions } = options;
  if (!view.logDir) return null;
  return (
    <div className="app-options-row app-log" data-ui="app.log">
      <Button variant="quiet" size="sm" icon="folder" className="app-log__open" data-ui="app.log.open" aria-describedby="app.log.help" onClick={actions.openLogFolder}>
        {t("app.log.open")}
      </Button>
      <p className="app-log__help" id="app.log.help">
        {t("app.log.help")}
      </p>
    </div>
  );
}
