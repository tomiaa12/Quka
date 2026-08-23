<script setup lang="ts">
import type { ScannerName } from "../services/app";

defineProps<{
  variant: "hint" | "empty" | "loading" | "scanning";
  scanner?: ScannerName;
}>();
</script>

<template>
  <div class="state">
    <div v-if="variant === 'loading' || variant === 'scanning'" class="spinner"></div>
    <div class="state-title">
      <template v-if="variant === 'scanning' && scanner === 'windows'">正在扫描 Windows 应用...</template>
      <template v-else-if="variant === 'scanning' && scanner === 'macos'">正在扫描 macOS 应用...</template>
      <template v-else-if="variant === 'scanning'">正在扫描应用...</template>
      <template v-else-if="variant === 'loading'">搜索中</template>
      <template v-else-if="variant === 'empty'">没有找到应用</template>
      <template v-else>搜索应用...</template>
    </div>
    <div class="state-desc">
      <template v-if="variant === 'scanning' && scanner === 'windows'">
        Start Menu · Program Files · LocalAppData\Programs
      </template>
      <template v-else-if="variant === 'scanning' && scanner === 'macos'">
        /Applications · ~/Applications · /System/Applications
      </template>
      <template v-else-if="variant === 'scanning'">正在索引本机已安装的软件</template>
      <template v-else-if="variant === 'loading'">正在匹配本地应用索引</template>
      <template v-else-if="variant === 'empty'">试试更短的关键词，或确认应用已经安装</template>
      <template v-else>输入名称即可查找并启动已安装的软件</template>
    </div>
  </div>
</template>
