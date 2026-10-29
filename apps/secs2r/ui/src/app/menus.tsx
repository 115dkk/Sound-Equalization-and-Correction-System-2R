// The project menu (새로 만들기, 열기, 저장, 다른 이름으로 저장) and the
// settings popover: the display settings (언어, 테마, 스킨, 로고), then the
// local API (its switch, its state and the 로컬 API dialog). Shortcut hints
// come from the shortcut registry.
import { useRef, useState } from "react";
import { useT } from "../i18n/react";
import { Icon } from "../ui/Icon";
import { Menu, Popover } from "../ui/Popover";
import { Segmented } from "../ui/controls";
import type { AppIcon } from "../ipc/bindings/AppIcon";
import { LocalApiDialog, LocalApiRow, type LocalApiFocus } from "./LocalApiDialog";
import { useLocalApi, type LocalApi } from "./localApi";
import { DEFAULT_LOGO, SMALL_LOGOS } from "./logo";
import { usePrefs, type SkinChoice, type ThemeChoice } from "./prefs";
import { shortcutText } from "./shortcuts";

export type ProjectActions = {
  onNew: () => void;
  onOpen: () => void;
  onSave: () => void;
  onSaveAs: () => void;
};

export function ProjectMenu({
  name,
  dirty,
  actions,
  initiallyOpen = false,
  inline = false,
}: {
  name: string | null;
  dirty: boolean;
  actions: ProjectActions;
  initiallyOpen?: boolean;
  inline?: boolean;
}) {
  const t = useT();
  const anchor = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(initiallyOpen);
  return (
    <div className="project-menu">
      <button
        ref={anchor}
        type="button"
        className="project-menu__button"
        aria-haspopup="menu"
        aria-expanded={open}
        data-ui="frame.project"
        id="frame.project"
        onClick={() => setOpen((value) => !value)}
      >
        <Icon name="file" size={18} className="project-menu__icon" />
        <span className="project-menu__text">
          <span className="project-menu__label">{t("frame.project")}</span>
          <span className="project-menu__name">
            {name ?? t("frame.status.file.unsaved")}
            {dirty && name && <span className="visually-hidden">{`, ${t("frame.status.file.changed")}`}</span>}
          </span>
        </span>
        <Icon name="chevronDown" size={16} className="project-menu__chevron" />
      </button>
      <Menu
        open={open}
        inline={inline}
        onClose={() => setOpen(false)}
        anchor={anchor}
        label={t("frame.project")}
        placement="bottom-start"
        id="frame.project.menu"
        items={[
          { id: "frame.project.new", label: t("frame.project.new"), icon: "plus", onSelect: actions.onNew },
          { id: "frame.project.open", label: t("frame.project.open"), icon: "folder", shortcut: shortcutText("project.open"), onSelect: actions.onOpen },
          { id: "frame.project.save", label: t("frame.project.save"), icon: "save", shortcut: shortcutText("project.save"), onSelect: actions.onSave },
          { id: "frame.project.save_as", label: t("frame.project.save_as"), shortcut: shortcutText("project.save_as"), onSelect: actions.onSaveAs },
        ]}
      />
    </div>
  );
}

export function SettingsButton({
  initiallyOpen = false,
  inline = false,
  localApi,
}: {
  initiallyOpen?: boolean;
  inline?: boolean;
  /** A fixed local API view and actions (gallery specimens, tests); the app reads the live ones. */
  localApi?: LocalApi;
}) {
  const t = useT();
  const prefs = usePrefs();
  const anchor = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(initiallyOpen);
  // The dialog replaces the popover: it opens as the popover closes, and
  // focus goes back to 설정 when it closes.
  const [dialog, setDialog] = useState<LocalApiFocus | null>(null);
  const live = useLocalApi(open || dialog !== null);
  const api = localApi ?? live;
  return (
    <>
      <button
        ref={anchor}
        type="button"
        className="rail-tool"
        aria-haspopup="dialog"
        aria-expanded={open}
        data-ui="frame.settings"
        id="frame.settings"
        onClick={() => setOpen((value) => !value)}
      >
        <Icon name="settings" size={18} />
        <span>{t("frame.settings")}</span>
      </button>
      <Popover open={open} inline={inline} onClose={() => setOpen(false)} anchor={anchor} label={t("frame.settings")} placement="right-start" id="frame.settings.panel">
        <h2 className="settings__title">{t("frame.settings")}</h2>
        <Segmented
          id="frame.settings.language"
          label={t("frame.settings.language")}
          value={prefs.locale}
          options={[
            { value: "ko", label: t("frame.settings.language.ko") },
            { value: "en", label: t("frame.settings.language.en") },
          ]}
          onChange={(locale) => prefs.set({ locale })}
        />
        <Segmented<ThemeChoice>
          id="frame.settings.theme"
          label={t("frame.settings.theme")}
          value={prefs.theme}
          options={[
            { value: "system", label: t("frame.settings.theme.system") },
            { value: "light", label: t("frame.settings.theme.light") },
            { value: "dark", label: t("frame.settings.theme.dark") },
          ]}
          onChange={(theme) => prefs.set({ theme })}
        />
        <Segmented<SkinChoice>
          id="frame.settings.skin"
          label={t("frame.settings.skin")}
          value={prefs.skin ?? "default"}
          options={[
            { value: "default", label: t("frame.settings.skin.default") },
            { value: "rack", label: t("frame.settings.skin.rack") },
            { value: "matrix", label: t("frame.settings.skin.matrix") },
          ]}
          onChange={(skin) => prefs.set({ skin })}
        />
        <Segmented<AppIcon>
          id="frame.settings.logo"
          label={t("frame.settings.logo")}
          value={prefs.icon ?? DEFAULT_LOGO}
          options={(["expert", "civilian"] as const).map((icon) => ({
            value: icon,
            label: t(`frame.settings.logo.${icon}`),
            media: <img className="settings__logo" src={SMALL_LOGOS[icon]} alt="" width={20} height={20} draggable={false} />,
          }))}
          onChange={(icon) => prefs.set({ icon })}
        />
        <LocalApiRow
          api={api}
          onOpen={(focus) => {
            setOpen(false);
            setDialog(focus);
          }}
        />
      </Popover>
      {dialog && <LocalApiDialog api={api} focus={dialog} onClose={() => setDialog(null)} />}
    </>
  );
}
