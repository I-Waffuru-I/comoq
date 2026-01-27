<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { connectionManager } from '../tools/ConnectionManager';
import { useRouter } from 'vue-router';
import ConnectionButtons from '@/elements/ConnectionButtons.vue';

const text = ref('');
const received = ref(false);

onMounted(() => {
  connectionManager.onMessage((t) => {
    text.value = t;
    received.value = true;
  });
});

function onInput() {
  if(received.value) {
    received.value = false;
    console.log("ignore other",text.value)
    return;
  }

  console.log("self",text.value)
  connectionManager.send(text.value);
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
