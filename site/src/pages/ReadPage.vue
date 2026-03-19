<script setup lang="ts">
import { ref, onMounted } from 'vue';
import ConnectionButtons from '../elements/ConnectionButtons.vue';
import { connectionManager } from '../tools/ConnectionManager';

const receivedText = ref('Waiting for messages...');
const counter = ref(0)

onMounted(() => {
  connectionManager.set_callback((text: string) => {
    receivedText.value = text;
  });
});
function update() {
  counter.value += 1;
  connectionManager.update(`${receivedText.value}${counter.value}`)
}
</script>

<template>
  <h2>Read Mode</h2>
  <div class="read-container">
    {{ receivedText }}
  </div>
  <button @click="update">update</button>
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
