import { defineStore } from "pinia";
import {
  changeGlobalShortcut,
  getSettings,
  nextShortcut,
  shortcutLabel,
  updateSettings,
} from "../services/settings";
import { applyTheme, nextTheme } from "../services/theme";
import type { SettingsState, ThemeMode } from "../types/settings";

const defaultSettings: SettingsState = {
  globalShortcut: "DoubleCtrl",
  launchAtStartup: false,
  resultLimit: 8,
  enableUsageRanking: true,
  theme: "system",
};

function snapshot(state: SettingsState): SettingsState {
  return {
    globalShortcut: state.globalShortcut,
    launchAtStartup: state.launchAtStartup,
    resultLimit: state.resultLimit,
    enableUsageRanking: state.enableUsageRanking,
    theme: state.theme,
  };
}

export const useSettingsStore = defineStore("settings", {
  state: (): SettingsState => ({ ...defaultSettings }),
  getters: {
    shortcutLabel: (state) => shortcutLabel(state.globalShortcut),
  },
  actions: {
    async load() {
      try {
        const remote = await getSettings();
        if (!remote) return;
        this.globalShortcut = remote.globalShortcut;
        this.launchAtStartup = remote.launchAtStartup;
        this.resultLimit = remote.resultLimit;
        this.enableUsageRanking = remote.enableUsageRanking;
        this.theme = remote.theme;
        applyTheme(this.theme);
      } catch (error) {
        console.error(error);
      }
    },
    async persist() {
      try {
        await updateSettings(snapshot(this));
      } catch (error) {
        console.error(error);
      }
    },
    async setTheme(theme: ThemeMode) {
      this.theme = theme;
      applyTheme(theme);
      await this.persist();
    },
    async cycleTheme() {
      await this.setTheme(nextTheme(this.theme));
    },
    async cycleShortcut() {
      const status = await changeGlobalShortcut(nextShortcut(this.globalShortcut));
      this.globalShortcut = status.shortcut;
      return status;
    },
    async setResultLimit(limit: number) {
      this.resultLimit = Math.min(12, Math.max(4, limit));
      await this.persist();
    },
    async setUsageRanking(enabled: boolean) {
      this.enableUsageRanking = enabled;
      await this.persist();
    },
    async setLaunchAtStartup(enabled: boolean) {
      const previous = this.launchAtStartup;
      this.launchAtStartup = enabled;
      try {
        await updateSettings(snapshot(this));
      } catch (error) {
        this.launchAtStartup = previous;
        throw error;
      }
    },
  },
});
