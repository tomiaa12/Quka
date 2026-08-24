import { t } from "../i18n";
import type { ThemeMode } from "../types/settings";

export function resolveTheme(theme: ThemeMode): "light" | "dark" {
  if (theme === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  return theme;
}

export function applyTheme(theme: ThemeMode): void {
  document.documentElement.dataset.theme = resolveTheme(theme);
}

export function themeLabel(theme: ThemeMode): string {
  if (theme === "system") return t("theme.system");
  if (theme === "light") return t("theme.light");
  return t("theme.dark");
}

export function nextTheme(theme: ThemeMode): ThemeMode {
  if (theme === "system") return "light";
  if (theme === "light") return "dark";
  return "system";
}
