<script setup lang="ts">
import {ref, defineProps, defineEmits} from "vue";

const props = defineProps({
   isConnected : Boolean,    
}); 

const emit = defineEmits([ "disconnect" ]);

const connectDisplay = ref("Disconnected")
</script>

<template>
   <button @click="toggleConnect()">{{connectDisplay}}</button>


</template>

<style>
</style>
