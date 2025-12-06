import { createRouter, createWebHistory } from 'vue-router'
import HomePage from "../pages/HomePage.vue";
import EditorPage from "../pages/EditorPage.vue";
import ConnectPage from "../pages/ConnectPage.vue";
import AppPage from "../pages/AppPage.vue";

const router = createRouter({
  history: createWebHistory(import.meta.env.BASE_URL),
  routes: [
     {
        path : "/",
        component: HomePage,
     },
     {
        path: "/app",
        component : AppPage,
        children : [
           {
              path : "connect",
              name : "connect",
              component: ConnectPage 
           },
           {
              path : "edit",
              name : "edit",
              component: EditorPage 
           },
        ]

     },
  ],
})

export default router

