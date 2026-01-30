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
    console.log("received: ["+val+"]")
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
  <h2>Read Mode</h2>
  <div class="read-container">
    <!-- <pre>{{ receivedText }}</pre> -->
    <textarea v-model="text" @input="onInput" placeholder="Type here to publish..."></textarea>
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
