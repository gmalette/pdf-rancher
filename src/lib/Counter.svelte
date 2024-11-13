<script lang="ts">
  import {invoke} from '@tauri-apps/api/core'
  import {info,} from '@tauri-apps/plugin-log';

  let count: number = $state(0)
  let src: string = $state("")

  const increment = () => {
    count += 1
    invoke("open_files").then((x: any) => {
      info("Log from Counter.svelte");
      info(JSON.stringify(x));
      // src = x.{"files":[{"pddath":"/Users/gmalette/Downloads/1Password Emergency Kit A3-9XL7PD-shopify.pdf","previews":[{"jpg_bytes":"/9j/4AAQSkZJRgABAgAAAQABAAD/wAARCAH0AYIDAREAAhEBAxEB/9sAQwAIBgYHBgUIBwcHCQkICgwUDQwLCwwZEhMPFB0aHx4
      src = "data:image/jpg;base64," + x.files[0].previews[0].jpg_bytes;
    });

  }
  info("Log from Counter.svelte");
</script>

<img {src}/>

<button onclick={increment}>
  count is {count}
</button>
