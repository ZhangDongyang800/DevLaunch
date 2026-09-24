<script setup lang="ts">
withDefaults(defineProps<{ size?: number }>(), { size: 20 })
</script>

<template>
  <svg
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    fill="none"
    aria-hidden="true"
  >
    <path d="M5 4.5h8.5L19 10v9.5H5z" stroke="currentColor" stroke-width="1.5" />
    <path d="M13.5 4.5V10H19" stroke="currentColor" stroke-width="1.5" />
    <path d="m9 13 5 2.8L9 18.6z" fill="currentColor" />
    <path d="M3 8v10" stroke="currentColor" stroke-width="1.5" />
  </svg>
</template>
