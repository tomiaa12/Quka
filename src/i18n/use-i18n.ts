import { translate, resolveLocale, type MessageKey } from "./index";
import { useSettingsStore } from "../stores/settings";

export function useI18n() {
  const settings = useSettingsStore();
  function t(key: MessageKey, params?: Record<string, string | number>): string {
    return translate(resolveLocale(settings.locale), key, params);
  }
  return { t };
}
