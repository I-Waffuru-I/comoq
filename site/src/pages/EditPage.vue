<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { useRouter } from 'vue-router';
import { connectionManager } from '../tools/ConnectionManager';
import ConnectionButtons from '@/elements/ConnectionButtons.vue';

const track = ref('text')
const text = ref('');
const received = ref(false);

onMounted(() => {
  connectionManager.onMessage((track, val) => {
    text.value = val;
    received.value = true;
  });
});

function onInput() {
  console.log("self",text.value)
  connectionManager.send(track.value, text.value);
}
</script>
<template>
  <h2>Edit Mode</h2>
  <div class="edit-container">
    <textarea v-model="text" @input="onInput" placeholder="Type here to publish..."></textarea>
  </div>
  <ConnectionButtons />
</template>
<style>
.edit-container {
  margin-bottom: 20px;
}
textarea {
  width: 100%;
  height: 200px;
}
</style>
