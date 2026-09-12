import { createApp } from "vue";
import "./style.css";
import App from "./App.vue";
import PaletteApp from "./PaletteApp.vue";

const isPalette = window.location.hash === "#palette";
createApp(isPalette ? PaletteApp : App).mount("#app");
