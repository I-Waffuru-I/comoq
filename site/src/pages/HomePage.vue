<script setup lang="ts">
import { ref } from 'vue';
import { useRouter } from 'vue-router';
import { connectionManager } from '../tools/ConnectionManager';

const router = useRouter();
const url = ref('http://localhost:4443/');
const namespace = ref('comoq');
const track = ref('text');

async function publish() {
  await connectionManager.startPublish(url.value, namespace.value, track.value);
  console.log("pre: ", connectionManager.isConnected())
  router.push({ name: 'edit' });
}

async function subscribe() {
  await connectionManager.startSubscribe(url.value, namespace.value, track.value);
  router.push({ name: 'read' });
}

</script>
<template>
  <h2>Connect</h2>

  <div>
    <label>URL: </label>
    <input v-model="url" type="text" placeholder="localhost:4443" />
  </div>
  <div>
    <label>Namespace: </label>
    <input v-model="namespace" type="text" placeholder="comoq" />
  </div>
  <div>
    <label>Track: </label>
    <input v-model="track" type="text" placeholder="text" />
  </div>

  <div style="margin-top: 10px;">
    <button @click="publish">Publish</button>
    <button @click="subscribe">Subscribe</button>
  </div>
</template>
<style>

</style>
