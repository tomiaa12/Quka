import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { isTauri } from "./window";

export type AppUpdate = {
  version: string;
  downloadAndInstall: () => Promise<void>;
};

let pending: Update | null = null;

export async function currentVersion(): Promise<string> {
  if (!isTauri()) return "0.1.0";
  return getVersion();
}

export async function checkForUpdate(): Promise<AppUpdate | null> {
  if (!isTauri()) return null;
  try {
    pending = await check();
    if (!pending) return null;
    const update = pending;
    return {
      version: update.version,
      downloadAndInstall: () => update.downloadAndInstall(),
    };
  } catch (error) {
    pending = null;
    console.error(error);
    throw error;
  }
}

export async function installUpdate(update: AppUpdate): Promise<void> {
  const target = pending ?? update;
  await target.downloadAndInstall();
  await relaunch();
}
