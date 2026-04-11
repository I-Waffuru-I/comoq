<script setup lang="ts">
import ConnectionButtons from '../elements/ConnectionButtons.vue';
import { ref, onMounted } from 'vue';
import { connectionManager } from '../tools/ConnectionManager';

const localText = ref('Waiting for messages...')

onMounted(() => {
  connectionManager.set_callback((text: string) => {
    localText.value = text;
  });
});
let debounceTimer: number | null = null;
function update() {
  if (debounceTimer) {
    clearTimeout(debounceTimer);
  }
  debounceTimer = window.setTimeout(() => {
    console.log(`update val ${localText.value}`)
    connectionManager.update(localText.value)
    debounceTimer = null;
  }, 300);
}

</script>

<template>
  <h2>Edit Mode</h2>
  <div class="edit-container">
    <textarea @input="update" v-model="localText" />

  </div>
  <ConnectionButtons />
</template>

<style>
.edit-container {
  margin-bottom: 20px;
}
</style>
<style>
.edit-container {
  margin-bottom: 20px;
}
textarea {
  width: 100%;
  height: 200px;
}
</style>
