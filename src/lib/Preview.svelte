<script lang="ts">
  import {previewToDataUrl} from "./project.js";
  import {tick} from "svelte";

  let { rotation, jpg, pageNum, fullSize }: { rotation: number, jpg: string, pageNum: number, fullSize?: boolean } = $props();
  fullSize = fullSize ?? false;

  let previewHtmlElement: Element;

  $effect.pre(() => {
    previewHtmlElement;

    tick().then(() => {
      if (previewHtmlElement) {
        const img = previewHtmlElement.querySelector('img')!;
        if (img.classList.contains('rotate90') || img.classList.contains('rotate270')) {
          previewHtmlElement.style.width = `${img.clientHeight}px`;
          previewHtmlElement.style.height = `${img.clientWidth}px`;
        } else {
          previewHtmlElement.style.width = null;
          previewHtmlElement.style.height = null;
        }
      }
    })
  });
</script>

<preview bind:this={previewHtmlElement} class:fullsize={fullSize}>
  <img src={previewToDataUrl(jpg)} alt="Page preview for page number {pageNum + 1}" class="rotate{rotation}" />
</preview>

<style>
    preview {
        display: flex;
        align-items: center;
        justify-content: center;
        height: var(--file-full-size-constraint);

        &:not(.fullsize) {
            height: var(--file-height);
            box-shadow: 2px 2px 4px rgba(0, 0, 0, 0.2);

            img {
                &.rotate90 { transform: rotate(90deg); max-height: unset; max-width: var(--file-height) }
                &.rotate180 { transform: rotate(180deg); }
                &.rotate270 { transform: rotate(270deg); max-height: unset; max-width: var(--file-height) }
            }
        }

        &.fullsize {
            img {
                &.rotate90 { transform: rotate(90deg); max-height: unset; max-width: var(--file-full-size-constraint) }
                &.rotate180 { transform: rotate(180deg); }
                &.rotate270 { transform: rotate(270deg); max-height: unset; max-width: var(--file-full-size-constraint) }
            }
        }
    }

    img {
        max-height: 100%;
        max-width: none;
        width: auto;
        transform-origin: center;
    }

</style>
