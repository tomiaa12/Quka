export type ThemeMode = "system" | "light" | "dark";

export interface SettingsState {
  globalShortcut: string;
  launchAtStartup: boolean;
  resultLimit: number;
  enableUsageRanking: boolean;
  theme: ThemeMode;
}
