<script setup lang="ts">
import {ref} from "vue";

const text = ref("text");
const textRows = ref(window.innerHeight / 20);
const handleClick = ()=>{
   console.log(text.value)
   console.log(window.innerHeight)
}
   
</script>

<template>

   <textarea v-model="text" class="text_input" :rows="textRows" />

</template>

<style scoped>

.text_input {
   width: 80%;
   bottom: 0;
}

</style>

