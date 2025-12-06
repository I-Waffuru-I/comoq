<script setup lang="ts">
import {ref} from "vue"
import TextDisplay from '../components/TextDisplay.vue'
import InteractionButtons from '../components/InteractionButtons.vue'
import {useRouter} from "vue-router"
import {connectionManager} from "../tools"


const router = useRouter()
   

const input = ref()
function tryConnect(){

   connectionManager.tryConnect()
   router.push({name :"edit"})
}
</script>

<template>
   <div class="pwrap">
      <input type="text" v-model="input" placeholder=""/>
      <button @click="tryConnect()">Connect</button>
   </div>

</template>

<style scoped>

.pwrap > * {
   width: 80%; 
}

</style>
