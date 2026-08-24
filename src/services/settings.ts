import { invoke } from "@tauri-apps/api/core";
import { t } from "../i18n";
import type { SettingsState } from "../types/settings";
import { isTauri } from "./window";

export interface ShortcutStatus {
  shortcut: string;
  label: string;
  registered: boolean;
  error: string;
}

export function shortcutLabel(value: string): string {
  const key = value.trim().toLowerCase().replace(/\s+/g, "");
  if (key === "doublealt" || key === "双击alt") return t("shortcut.alt");
  if (key === "doublecommand" || key === "doublewin" || key === "双击command" || key === "双击win") {
    return navigator.userAgent.includes("Mac") ? t("shortcut.command") : t("shortcut.win");
  }
  return t("shortcut.ctrl");
}

export function nextShortcut(value: string): string {
  const key = value.trim().toLowerCase().replace(/\s+/g, "");
  if (key === "doublectrl" || key === "双击ctrl") return "DoubleAlt";
  if (key === "doublealt" || key === "双击alt") return "DoubleCommand";
  return "DoubleCtrl";
}

export async function getSettings(): Promise<SettingsState | null> {
  if (!isTauri()) return null;
  return invoke("get_settings");
}

export async function updateSettings(settings: SettingsState): Promise<void> {
  if (!isTauri()) return;
  await invoke("update_settings", { settings });
}

export async function getShortcutStatus(): Promise<ShortcutStatus> {
  if (!isTauri()) {
    return { shortcut: "DoubleCtrl", label: shortcutLabel("DoubleCtrl"), registered: false, error: "" };
  }
  return invoke("get_shortcut_status");
}

export async function changeGlobalShortcut(shortcut: string): Promise<ShortcutStatus> {
  if (!isTauri()) {
    return { shortcut, label: shortcutLabel(shortcut), registered: false, error: "" };
  }
  return invoke("change_global_shortcut", { shortcut });
}
