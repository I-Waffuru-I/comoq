import { createRouter, createWebHistory } from 'vue-router'
import HomePage from "../pages/HomePage.vue"
import EditPage from "../pages/EditPage.vue"
import ReadPage from "../pages/ReadPage.vue"

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
      {
        path: '/', name: 'home', component: HomePage
      },
      {
        path: '/read', name : 'read', component: ReadPage
      },
      {
        path: '/edit', name : 'edit', component: EditPage
      },
  ],
})

export default router
