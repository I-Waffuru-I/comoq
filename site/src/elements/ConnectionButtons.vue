<script setup lang="ts">
import { useRouter } from 'vue-router';
import { connectionManager } from '../tools/ConnectionManager';

const router = useRouter();

function disconnect() {
  connectionManager.disconnect();
  router.push({ name: 'home' });
}
</script>
<template>
  <div class="connection-buttons">
    <button @click="disconnect">Disconnect</button>
  </div>
</template>
<style>
.connection-buttons {
  margin-top: 20px;
  border-top: 1px solid #eee;
  padding-top: 10px;
}
</style>
