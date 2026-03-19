<script setup lang="ts">
import ConnectionButtons from '../elements/ConnectionButtons.vue';
import { ref, onMounted } from 'vue';
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
  <h2>Edit Mode</h2>
  <div class="edit-container">
    <textarea />

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
