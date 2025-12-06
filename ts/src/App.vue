<script setup lang="ts">

</script>

<template>
   <h2>IndiP : MoQ code thingy</h2>
   <nav>
      <RouterLink to="/">Home</RouterLink>
      <RouterLink to="/app">Get started</RouterLink>
   </nav>
   <RouterView />
</template>

<style scoped>
nav {
   margin-bottom:10px;
}
nav > * {
   margin-left: 10px;
}
nav > * + * {
   padding-left: 10px;
}

</style>
