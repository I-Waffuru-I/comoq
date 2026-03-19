<script setup lang="ts">
import { ref } from 'vue';
import { useRouter } from 'vue-router';
import { connectionManager } from '../tools/ConnectionManager';

const router = useRouter();
const url = ref('http://localhost:4443');

async function read() {
  await connectionManager.run(url.value);
  router.push({ name: 'read' });
}
async function write() {
  await connectionManager.run(url.value);
  router.push({ name: 'edit' });
}


</script>
<template>
  <h2>Connect</h2>

  <div>
    <label>URL: </label>
    <input v-model="url" type="text" placeholder="https://localhost:4443" />
  </div>

  <div style="margin-top: 10px;">
    <button @click="connect">Read</button>
    <button @click="connect">Edit</button>
  </div>
</template>
<style>

</style>
