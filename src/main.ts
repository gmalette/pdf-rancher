import {mount} from 'svelte'
import './app.css'
import './assets/css/brands.css'
import './assets/css/fontawesome.css'
import './assets/css/regular.css'
import './assets/css/solid.css'
import App from './App.svelte'
import {attachConsole} from "@tauri-apps/plugin-log";

attachConsole();

const app = mount(App, {
  target: document.getElementById('app')!,
})

export default app
