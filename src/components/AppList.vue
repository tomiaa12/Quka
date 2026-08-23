<script setup lang="ts">
import type { Application } from "../types/application";
import AppItem from "./AppItem.vue";

defineProps<{
  apps: Application[];
  selectedIndex: number;
  keyword: string;
  recent: boolean;
  elapsedMs?: number;
}>();

const emit = defineEmits<{
  select: [index: number];
  launch: [];
}>();
</script>

<template>
  <div class="app-list">
    <AppItem
      v-for="(app, index) in apps"
      :key="app.id"
      :app="app"
      :selected="index === selectedIndex"
      :keyword="keyword"
      :recent="recent"
      :elapsed-ms="index === selectedIndex ? elapsedMs : 0"
      @select="emit('select', index)"
      @launch="emit('launch')"
    />
  </div>
</template>
