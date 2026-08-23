import { invoke } from "@tauri-apps/api/core";
import { MOCK_APPLICATIONS } from "../data/mock-applications";
import type { Application } from "../types/application";
import { isTauri } from "./window";

export type ScannerName = "macos" | "windows" | "none";

export interface DatabaseInfo {
  applicationCount: number;
  justInitialized: boolean;
  needsScan: boolean;
  scanner: ScannerName;
}

export interface ScanResult {
  applicationCount: number;
  inserted: number;
  updated: number;
  deleted: number;
  supported: boolean;
  scanner: ScannerName;
}

export async function getApplications(): Promise<Application[]> {
  if (!isTauri()) {
    return MOCK_APPLICATIONS.map((app) => ({ ...app }));
  }
  return invoke("get_applications");
}

export async function updateApplication(application: Application): Promise<void> {
  if (!isTauri()) return;
  await invoke("update_application", { application });
}

export async function getDatabaseInfo(): Promise<DatabaseInfo> {
  if (!isTauri()) {
    return {
      applicationCount: MOCK_APPLICATIONS.length,
      justInitialized: false,
      needsScan: false,
      scanner: "none",
    };
  }
  return invoke("get_database_info");
}

export async function rescanApplications(): Promise<ScanResult> {
  if (!isTauri()) {
    return {
      applicationCount: MOCK_APPLICATIONS.length,
      inserted: 0,
      updated: 0,
      deleted: 0,
      supported: false,
      scanner: "none",
    };
  }
  return invoke("rescan_applications");
}

export async function launchApplication(id: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("launch_application", { id });
}
