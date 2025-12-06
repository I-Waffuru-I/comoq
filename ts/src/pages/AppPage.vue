<script setup lang="ts">
import {connectionManager} from "../tools"
import {ref, onMounted} from "vue"
import {useRouter} from "vue-router"


const router = useRouter()


onMounted(()=>{
   if(!connectionManager.isConnected()) {
      router.push({name: "connect"})
   }
})

</script>

<template>
   <h3>Application :)</h3> 

   <RouterView />
</template>


<style scoped>

</style>



