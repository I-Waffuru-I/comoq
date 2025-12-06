<script setup lang="ts">
import {ref, defineProps} from "vue"
import TextDisplay from '../components/TextDisplay.vue'
import InteractionButtons from '../components/InteractionButtons.vue'
import {connectionManager} from "../tools"

const props = defineProps({
     
});

</script>

<template>
   <div class="pwrap">
      <InteractionButtons :isConnected="connectionManager.isConnected()" />
      <TextDisplay  />
   </div>

</template>

<style scoped>

.pwrap > * {
   width: 80%; 
}

</style>
