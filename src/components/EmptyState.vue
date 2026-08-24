<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "../i18n/use-i18n";
import type { ScannerName } from "../services/app";

const props = defineProps<{
  variant: "hint" | "empty" | "loading" | "scanning";
  scanner?: ScannerName;
}>();

const { t } = useI18n();

const title = computed(() => {
  if (props.variant === "scanning" && props.scanner === "windows") return t("empty.scanningWindows");
  if (props.variant === "scanning" && props.scanner === "macos") return t("empty.scanningMac");
  if (props.variant === "scanning") return t("empty.scanning");
  if (props.variant === "loading") return t("empty.loading");
  if (props.variant === "empty") return t("empty.none");
  return t("empty.hint");
});

const desc = computed(() => {
  if (props.variant === "scanning" && props.scanner === "windows") return t("empty.scanningWindowsDesc");
  if (props.variant === "scanning" && props.scanner === "macos") return t("empty.scanningMacDesc");
  if (props.variant === "scanning") return t("empty.scanningDesc");
  if (props.variant === "loading") return t("empty.loadingDesc");
  if (props.variant === "empty") return t("empty.noneDesc");
  return t("empty.hintDesc");
});
</script>

<template>
  <div class="state">
    <div v-if="variant === 'loading' || variant === 'scanning'" class="spinner"></div>
    <div class="state-title">{{ title }}</div>
    <div class="state-desc">{{ desc }}</div>
  </div>
</template>
