import { createRouter, createWebHashHistory } from "vue-router";
import XiaomiSettings from "../views/XiaomiSettings.vue";
import GlobalSettings from "../views/GlobalSettings.vue";
import HisenseSettings from "../views/HisenseSettings.vue";

const router = createRouter({
  history: createWebHashHistory(),
  scrollBehavior(to) { return to.hash ? { el: to.hash, top: 20, behavior: "smooth" } : { top: 0 }; },
  routes: [
    { path: "/hisense", name: "hisense", component: HisenseSettings },
    {
      path: "/",
      redirect: "/xiaomi",
    },
    {
      path: "/xiaomi",
      name: "xiaomi",
      component: XiaomiSettings,
    },
    {
      path: "/xiaomi/mapping",
      name: "xiaomi-mapping",
      redirect: { path: "/xiaomi", hash: "#xiaomi-mapping" },
    },
    {
      path: "/settings",
      name: "settings",
      component: GlobalSettings,
    },
    {
      path: "/:pathMatch(.*)*",
      redirect: "/xiaomi",
    },
  ],
});

export default router;
