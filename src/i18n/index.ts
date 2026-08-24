import { en, zhCN, type MessageKey } from "./messages";
import type { LocaleMode } from "../types/settings";

export type { MessageKey };

export type { LocaleMode };
export type ResolvedLocale = "zh-CN" | "en";

const tables: Record<ResolvedLocale, Record<MessageKey, string>> = {
  "zh-CN": zhCN,
  en,
};

let activeLocale: ResolvedLocale = detectSystemLocale();

export function detectSystemLocale(): ResolvedLocale {
  const languages = [navigator.language, ...(navigator.languages ?? [])];
  return languages.some((item) => item.toLowerCase().startsWith("zh")) ? "zh-CN" : "en";
}

export function resolveLocale(mode: LocaleMode): ResolvedLocale {
  if (mode === "zh-CN") return "zh-CN";
  if (mode === "en") return "en";
  return detectSystemLocale();
}

export function applyLocale(mode: LocaleMode): ResolvedLocale {
  activeLocale = resolveLocale(mode);
  document.documentElement.lang = activeLocale;
  return activeLocale;
}

export function currentLocale(): ResolvedLocale {
  return activeLocale;
}

export function translate(
  locale: ResolvedLocale,
  key: MessageKey,
  params?: Record<string, string | number>,
): string {
  const template = tables[locale][key] ?? tables.en[key] ?? key;
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (_, name: string) => String(params[name] ?? ""));
}

export function t(key: MessageKey, params?: Record<string, string | number>): string {
  return translate(activeLocale, key, params);
}

export function localeLabel(mode: LocaleMode): string {
  if (mode === "zh-CN") return t("locale.zhCN");
  if (mode === "en") return t("locale.en");
  return t("locale.system");
}

export function nextLocale(mode: LocaleMode): LocaleMode {
  if (mode === "system") return "zh-CN";
  if (mode === "zh-CN") return "en";
  return "system";
}
