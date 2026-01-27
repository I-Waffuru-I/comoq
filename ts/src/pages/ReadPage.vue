<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import { connectionManager } from '../tools/ConnectionManager';
import ConnectionButtons from '../elements/ConnectionButtons.vue';

const receivedText = ref('');

onMounted(() => {
  connectionManager.onMessage((text) => {
    receivedText.value = text;
  });
});

</script>

<template>
  <h2>Read Mode</h2>
  <div class="read-container">
    <pre>{{ receivedText }}</pre>
  </div>
  <ConnectionButtons />
</template>

<style>
.read-container {
  border: 1px solid #ccc;
  padding: 10px;
  min-height: 200px;
  margin-bottom: 20px;
  white-space: pre-wrap;
}
</style>
