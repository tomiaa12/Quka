<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "../i18n/use-i18n";
import { appIconImageSrc, appIconSvg, isCachedIconPath } from "../services/icon";
import { highlightName } from "../services/search";
import type { Application } from "../types/application";

const { t } = useI18n();

const props = defineProps<{
  app: Application;
  selected: boolean;
  keyword: string;
  recent: boolean;
  elapsedMs?: number;
}>();

const emit = defineEmits<{
  select: [];
  launch: [];
}>();

const imageSrc = computed(() =>
  isCachedIconPath(props.app.icon) && props.app.icon ? appIconImageSrc(props.app.icon) : "",
);
const icon = computed(() => appIconSvg(props.app.icon));
const parts = computed(() => highlightName(props.app.name, props.keyword));
const subtitle = computed(() => {
  if (props.recent) return t("app.recent");
  if (props.app.bundleId) return `${props.app.path} · ${props.app.bundleId}`;
  return props.app.path;
});
const meta = computed(() => {
  if (props.selected && !props.recent && props.elapsedMs && props.elapsedMs > 0) {
    return `${props.elapsedMs} ms`;
  }
  return props.selected ? t("app.launchEnter") : t("app.app");
});
</script>

<template>
  <button
    type="button"
    class="app-item"
    :class="{ 'is-selected': selected }"
    @click="emit('select')"
    @dblclick="emit('launch')"
  >
    <div class="app-icon">
      <img v-if="imageSrc" :src="imageSrc" alt="" />
      <div v-else v-html="icon"></div>
    </div>
    <div class="app-body">
      <div class="app-name">
        <template v-for="(part, index) in parts" :key="index">
          <mark v-if="part.match">{{ part.text }}</mark>
          <span v-else>{{ part.text }}</span>
        </template>
      </div>
      <div class="app-sub">{{ subtitle }}</div>
    </div>
    <div class="app-meta">{{ meta }}</div>
  </button>
</template>
