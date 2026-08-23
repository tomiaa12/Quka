<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getDatabaseInfo, rescanApplications, type ScanResult } from "../services/app";
import { checkForUpdate, currentVersion, installUpdate, type AppUpdate } from "../services/update";
import { isTauri } from "../services/window";
import { themeLabel } from "../services/theme";
import { useSettingsStore } from "../stores/settings";

type SettingsTab = "general" | "search" | "apps";

const settings = useSettingsStore();
const tab = ref<SettingsTab>("general");
const appCount = ref(0);
const scanning = ref(false);
const message = ref("");
const error = ref("");
const version = ref("");
const updateInfo = ref<AppUpdate | null>(null);
const checkingUpdate = ref(false);
const installingUpdate = ref(false);

let unlistenRescan: UnlistenFn | undefined;

function setTab(next: SettingsTab): void {
  tab.value = next;
}

async function refreshCount(): Promise<void> {
  try {
    const info = await getDatabaseInfo();
    appCount.value = info.applicationCount;
  } catch (item) {
    error.value = item instanceof Error ? item.message : String(item);
  }
}

async function toggleStartup(): Promise<void> {
  try {
    const next = !settings.launchAtStartup;
    await settings.setLaunchAtStartup(next);
    error.value = "";
    message.value = next ? "已开启开机启动" : "已关闭开机启动";
  } catch (item) {
    error.value = item instanceof Error ? item.message : String(item);
  }
}

async function cycleShortcut(): Promise<void> {
  try {
    const status = await settings.cycleShortcut();
    if (status.error || !status.registered) {
      error.value = status.error || "快捷键注册失败";
      return;
    }
    error.value = "";
    message.value = `快捷键已改为 ${status.label}`;
  } catch (item) {
    error.value = item instanceof Error ? item.message : String(item);
  }
}

async function applyScan(result: ScanResult): Promise<void> {
  appCount.value = result.applicationCount;
  message.value = `已扫描 ${result.applicationCount} 个应用，新增 ${result.inserted}，更新 ${result.updated}`;
}

function updateStatus(): string {
  if (installingUpdate.value) return "正在下载并安装更新…";
  if (checkingUpdate.value) return "正在检查更新…";
  if (updateInfo.value) return `发现新版本 ${updateInfo.value.version}`;
  if (version.value) return `当前 ${version.value}`;
  return "检查 GitHub Release";
}

async function checkUpdate(silent = false): Promise<void> {
  checkingUpdate.value = true;
  try {
    const next = await checkForUpdate();
    updateInfo.value = next;
    if (!silent) {
      error.value = "";
      message.value = next ? `发现新版本 ${next.version}` : "已是最新版本";
    }
  } catch (item) {
    updateInfo.value = null;
    if (!silent) {
      const text = item instanceof Error ? item.message : String(item);
      error.value = text.includes("Could not fetch") || text.includes("error sending request")
        ? "检查更新失败，请确认已发布 Release"
        : text;
    }
  } finally {
    checkingUpdate.value = false;
  }
}

async function applyUpdate(): Promise<void> {
  if (!updateInfo.value) return;
  installingUpdate.value = true;
  error.value = "";
  try {
    await installUpdate(updateInfo.value);
  } catch (item) {
    error.value = item instanceof Error ? item.message : String(item);
    installingUpdate.value = false;
  }
}

async function rescan(): Promise<void> {
  scanning.value = true;
  error.value = "";
  try {
    await applyScan(await rescanApplications());
  } catch (item) {
    error.value = item instanceof Error ? item.message : String(item);
  } finally {
    scanning.value = false;
  }
}

onMounted(() => {
  void settings.load();
  void refreshCount();
  void currentVersion().then((value) => {
    version.value = value;
  });
  if (!isTauri()) return;
  void checkUpdate(true);
  void listen<ScanResult>("apps-rescanned", (event) => {
    void applyScan(event.payload);
  }).then((unlisten) => {
    unlistenRescan = unlisten;
  });
});

onUnmounted(() => {
  void unlistenRescan?.();
});
</script>

<template>
  <section class="settings-win">
    <div class="settings-body">
      <nav class="settings-nav">
        <button type="button" class="nav-item" :class="{ 'is-active': tab === 'general' }" @click="setTab('general')">
          通用
        </button>
        <button type="button" class="nav-item" :class="{ 'is-active': tab === 'search' }" @click="setTab('search')">
          搜索
        </button>
        <button type="button" class="nav-item" :class="{ 'is-active': tab === 'apps' }" @click="setTab('apps')">
          应用
        </button>
      </nav>
      <main class="settings-main">
        <div v-if="error" class="banner banner-error">{{ error }}</div>
        <div v-else-if="message" class="banner banner-ok">{{ message }}</div>

        <template v-if="tab === 'general'">
          <h2>通用</h2>
          <div class="row">
            <div>
              <div class="row-title">开机启动</div>
              <div class="row-desc">登录系统后在后台运行。Windows Startup / macOS Login Item</div>
            </div>
            <button
              type="button"
              class="toggle"
              :class="{ on: settings.launchAtStartup }"
              @click="toggleStartup"
            >
              <i></i>
            </button>
          </div>
          <div class="row">
            <div>
              <div class="row-title">全局快捷键</div>
              <div class="row-desc">呼出搜索窗口。修改后会注销旧快捷键并重新注册</div>
            </div>
            <button type="button" class="shortcut-box" @click="cycleShortcut">{{ settings.shortcutLabel }}</button>
          </div>
          <div class="row">
            <div>
              <div class="row-title">主题</div>
              <div class="row-desc">跟随系统，也可固定 Light / Dark</div>
            </div>
            <button type="button" class="shortcut-box" @click="settings.cycleTheme()">
              {{ themeLabel(settings.theme) }}
            </button>
          </div>
          <div class="row">
            <div>
              <div class="row-title">软件更新</div>
              <div class="row-desc">{{ updateStatus() }}</div>
            </div>
            <button
              v-if="updateInfo"
              type="button"
              class="btn"
              :disabled="installingUpdate"
              @click="applyUpdate"
            >
              {{ installingUpdate ? "更新中…" : "立即更新" }}
            </button>
            <button
              v-else
              type="button"
              class="btn"
              :disabled="checkingUpdate"
              @click="checkUpdate(false)"
            >
              {{ checkingUpdate ? "检查中…" : "检查更新" }}
            </button>
          </div>
        </template>

        <template v-else-if="tab === 'search'">
          <h2>搜索</h2>
          <div class="row">
            <div>
              <div class="row-title">最大结果数量</div>
              <div class="row-desc">搜索窗口最多显示 4～12 项，高度随结果变化</div>
            </div>
            <div class="stepper">
              <button type="button" class="btn" @click="settings.setResultLimit(settings.resultLimit - 1)">−</button>
              <b>{{ settings.resultLimit }}</b>
              <button type="button" class="btn" @click="settings.setResultLimit(settings.resultLimit + 1)">+</button>
            </div>
          </div>
          <div class="row">
            <div>
              <div class="row-title">使用频率排序</div>
              <div class="row-desc">按启动次数与最近启动时间提升常用应用</div>
            </div>
            <button
              type="button"
              class="toggle"
              :class="{ on: settings.enableUsageRanking }"
              @click="settings.setUsageRanking(!settings.enableUsageRanking)"
            >
              <i></i>
            </button>
          </div>
        </template>

        <template v-else>
          <h2>应用</h2>
          <div class="row">
            <div>
              <div class="row-title">重新扫描</div>
              <div class="row-desc">增量扫描已安装应用，不删除用户设置</div>
            </div>
            <button type="button" class="btn" :disabled="scanning" @click="rescan">
              {{ scanning ? "扫描中…" : "重新扫描" }}
            </button>
          </div>
          <div class="row">
            <div>
              <div class="row-title">本地应用库</div>
              <div class="row-desc">当前已索引 {{ appCount }} 个应用</div>
            </div>
            <span class="app-meta">SQLite</span>
          </div>
        </template>
      </main>
    </div>
  </section>
</template>
