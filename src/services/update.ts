import { getVersion } from "@tauri-apps/api/app";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { isTauri } from "./window";

export type AppUpdate = {
  version: string;
  downloadAndInstall: () => Promise<void>;
};

export type UpdateProgress = {
  downloaded: number;
  total: number;
  percent: number;
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

export async function installUpdate(
  update: AppUpdate,
  onProgress?: (progress: UpdateProgress) => void,
): Promise<void> {
  const target = pending ?? update;
  let downloaded = 0;
  let total = 0;
  await target.downloadAndInstall((event) => {
    if (event.event === "Started") {
      total = event.data.contentLength ?? 0;
      downloaded = 0;
      onProgress?.({ downloaded, total, percent: 0 });
      return;
    }
    if (event.event === "Progress") {
      downloaded += event.data.chunkLength;
      const percent = total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : 0;
      onProgress?.({ downloaded, total, percent });
      return;
    }
    if (event.event === "Finished") {
      onProgress?.({ downloaded: total || downloaded, total, percent: total > 0 ? 100 : 0 });
    }
  });
  await relaunch();
}
