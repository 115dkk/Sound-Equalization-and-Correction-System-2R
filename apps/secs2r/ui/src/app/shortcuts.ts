// The one keyboard shortcut registry (upstream window.py:333-348). Menus
// and tooltips read their hints from here, and one listener dispatches
// them. Mod is Ctrl on Windows and Linux, ⌘ on macOS.
import { useEffect, useRef } from "react";
import type { MessageKey } from "../i18n";

export type ShortcutId =
  | "step.prev"
  | "step.next"
  | "step.1"
  | "step.2"
  | "step.3"
  | "step.4"
  | "step.5"
  | "project.save"
  | "project.save_as"
  | "project.open"
  | "help";

/** Ctrl+1…5 in rail order. */
export const STEP_SHORTCUTS = ["step.1", "step.2", "step.3", "step.4", "step.5"] as const satisfies readonly ShortcutId[];

export type Shortcut = {
  id: ShortcutId;
  key: string;
  mod?: boolean;
  shift?: boolean;
  alt?: boolean;
  label: MessageKey;
};

export const SHORTCUTS: readonly Shortcut[] = [
  { id: "step.prev", key: "ArrowLeft", alt: true, label: "shortcut.step_prev" },
  { id: "step.next", key: "ArrowRight", alt: true, label: "shortcut.step_next" },
  { id: "step.1", key: "1", mod: true, label: "shortcut.step_n" },
  { id: "step.2", key: "2", mod: true, label: "shortcut.step_n" },
  { id: "step.3", key: "3", mod: true, label: "shortcut.step_n" },
  { id: "step.4", key: "4", mod: true, label: "shortcut.step_n" },
  { id: "step.5", key: "5", mod: true, label: "shortcut.step_n" },
  { id: "project.save", key: "s", mod: true, label: "frame.project.save" },
  { id: "project.save_as", key: "s", mod: true, shift: true, label: "frame.project.save_as" },
  { id: "project.open", key: "o", mod: true, label: "frame.project.open" },
  { id: "help", key: "F1", label: "frame.help" },
];

export type KeyLike = { key: string; code?: string; ctrlKey: boolean; metaKey: boolean; altKey: boolean; shiftKey: boolean };

export const isMac = (): boolean => typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);

/** The shortcut a key event triggers, or null. Modifiers must match exactly. */
export function matchShortcut(event: KeyLike, mac = isMac()): ShortcutId | null {
  const mod = mac ? event.metaKey : event.ctrlKey;
  const stray = mac ? event.ctrlKey : event.metaKey;
  if (stray) return null;
  // Digits with Shift or a non-Latin layout still report the physical key in `code`.
  const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;
  const codeKey = event.code?.startsWith("Digit") ? event.code.slice(5) : event.code?.startsWith("Key") ? event.code.slice(3).toLowerCase() : undefined;
  for (const shortcut of SHORTCUTS) {
    if (!!shortcut.mod !== mod || !!shortcut.shift !== event.shiftKey || !!shortcut.alt !== event.altKey) continue;
    if (shortcut.key === key || (codeKey !== undefined && shortcut.key === codeKey)) return shortcut.id;
  }
  return null;
}

const ARROWS: Record<string, string> = { ArrowLeft: "←", ArrowRight: "→" };

/** "Ctrl+Shift+S", "Alt+←", "F1" (or "⌘⇧S" on macOS). */
export function shortcutText(id: ShortcutId, mac = isMac()): string {
  const shortcut = SHORTCUTS.find((s) => s.id === id);
  if (!shortcut) return "";
  const key = ARROWS[shortcut.key] ?? (shortcut.key.length === 1 ? shortcut.key.toUpperCase() : shortcut.key);
  if (mac) return `${shortcut.mod ? "⌘" : ""}${shortcut.alt ? "⌥" : ""}${shortcut.shift ? "⇧" : ""}${key}`;
  return [shortcut.mod && "Ctrl", shortcut.alt && "Alt", shortcut.shift && "Shift", key].filter(Boolean).join("+");
}

/** Installs one window listener for the handlers given; later handlers replace earlier ones. */
export function useShortcuts(handlers: Partial<Record<ShortcutId, () => void>>, enabled = true): void {
  const current = useRef(handlers);
  current.current = handlers;
  useEffect(() => {
    if (!enabled) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.isComposing) return;
      const id = matchShortcut(event);
      const handler = id ? current.current[id] : undefined;
      if (id && handler) {
        event.preventDefault();
        handler();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [enabled]);
}
