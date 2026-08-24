export type ThemeMode = "system" | "light" | "dark";
export type LocaleMode = "system" | "zh-CN" | "en";

export interface SettingsState {
  globalShortcut: string;
  launchAtStartup: boolean;
  resultLimit: number;
  enableUsageRanking: boolean;
  theme: ThemeMode;
  locale: LocaleMode;
}
