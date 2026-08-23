import { defineStore } from "pinia";
import {
  getDatabaseInfo,
  launchApplication,
  rescanApplications,
  type ScannerName,
} from "../services/app";
import { searchApplications } from "../services/search";
import { hideSearchWindow } from "../services/window";
import type { Application } from "../types/application";

const SEARCH_DELAY_MS = 16;

export const useSearchStore = defineStore("search", {
  state: () => ({
    keyword: "",
    results: [] as Application[],
    selectedIndex: 0,
    loading: false,
    error: "",
    notice: "",
    scanning: false,
    scanner: "none" as ScannerName,
    searchTimer: 0,
    searchElapsedMs: 0,
  }),
  getters: {
    selected(): Application | null {
      return this.results[this.selectedIndex] ?? null;
    },
    isEmptyKeyword(): boolean {
      return this.keyword.trim().length === 0;
    },
  },
  actions: {
    async bootstrap() {
      try {
        const info = await getDatabaseInfo();
        this.scanner = info.scanner;
        if (info.needsScan) {
          this.scanning = true;
          try {
            const result = await rescanApplications();
            this.notice = `发现 ${result.applicationCount} 个应用`;
          } finally {
            this.scanning = false;
          }
        } else if (info.justInitialized) {
          this.notice = `发现 ${info.applicationCount} 个应用，已写入本地 SQLite`;
        }
      } catch (error) {
        this.scanning = false;
        this.error = error instanceof Error ? error.message : String(error);
      }
      await this.search();
    },
    async search() {
      this.loading = true;
      const started = performance.now();
      try {
        this.results = await searchApplications(this.keyword);
        this.searchElapsedMs = Math.max(0, Math.round(performance.now() - started));
        this.error = "";
        if (this.selectedIndex >= this.results.length) {
          this.selectedIndex = Math.max(this.results.length - 1, 0);
        }
      } catch (error) {
        this.error = error instanceof Error ? error.message : String(error);
        this.results = [];
        this.searchElapsedMs = 0;
      } finally {
        this.loading = false;
      }
    },
    setKeyword(value: string) {
      this.keyword = value;
      this.error = "";
      this.notice = "";
      this.loading = value.trim().length > 0;
      window.clearTimeout(this.searchTimer);
      this.searchTimer = window.setTimeout(() => {
        void this.search();
      }, SEARCH_DELAY_MS);
    },
    moveUp() {
      if (this.results.length === 0) return;
      this.selectedIndex = (this.selectedIndex - 1 + this.results.length) % this.results.length;
    },
    moveDown() {
      if (this.results.length === 0) return;
      this.selectedIndex = (this.selectedIndex + 1) % this.results.length;
    },
    selectIndex(index: number) {
      this.selectedIndex = index;
    },
    async launchSelected() {
      const app = this.selected;
      if (!app) return;

      try {
        await launchApplication(app.id);
        this.clear();
        await hideSearchWindow();
      } catch (error) {
        this.error = error instanceof Error ? error.message : String(error);
      }
    },
    clear() {
      window.clearTimeout(this.searchTimer);
      this.keyword = "";
      this.error = "";
      this.loading = false;
      this.selectedIndex = 0;
      void this.search();
    },
  },
});
