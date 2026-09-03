export type ThemeMode = "system" | "light" | "dark";
export type LocaleMode = "system" | "zh-CN" | "en";
export type TrayIconStyle = "color" | "mono" | "search" | "bolt";

export interface SettingsState {
  globalShortcut: string;
  launchAtStartup: boolean;
  resultLimit: number;
  enableUsageRanking: boolean;
  theme: ThemeMode;
  locale: LocaleMode;
  disableOnFullscreen: boolean;
  trayIcon: TrayIconStyle;
}
