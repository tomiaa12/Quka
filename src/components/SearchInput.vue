<script setup lang="ts">
import { nextTick, onMounted, ref } from "vue";

const props = defineProps<{
  modelValue: string;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: string];
  keydown: [event: KeyboardEvent];
}>();

const inputRef = ref<HTMLInputElement | null>(null);

function focus(): void {
  inputRef.value?.focus();
}

function onInput(event: Event): void {
  const target = event.target as HTMLInputElement;
  emit("update:modelValue", target.value);
}

function onBlur(): void {
  void nextTick(() => focus());
}

onMounted(() => {
  focus();
});

defineExpose({ focus });
</script>

<template>
  <div class="search-input-row" data-tauri-drag-region>
    <svg class="search-glyph" viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <circle cx="11" cy="11" r="6.5" stroke="currentColor" stroke-width="1.8" />
      <path d="m16 16 4 4" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" />
    </svg>
    <input
      ref="inputRef"
      class="search-field"
      :value="props.modelValue"
      type="text"
      placeholder="搜索应用..."
      spellcheck="false"
      autocomplete="off"
      @input="onInput"
      @keydown="emit('keydown', $event)"
      @blur="onBlur"
    />
  </div>
</template>
