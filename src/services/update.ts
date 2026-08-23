import { getVersion } from "@tauri-apps/api/app";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { isTauri } from "./window";

export type AppUpdate = {
  version: string;
  downloadAndInstall: () => Promise<void>;
};

export async function currentVersion(): Promise<string> {
  if (!isTauri()) return "0.1.0";
  return getVersion();
}

export async function checkForUpdate(): Promise<AppUpdate | null> {
  if (!isTauri()) return null;
  try {
    return await check();
  } catch (error) {
    console.error(error);
    throw error;
  }
}

export async function installUpdate(update: AppUpdate): Promise<void> {
  await update.downloadAndInstall();
  await relaunch();
}
