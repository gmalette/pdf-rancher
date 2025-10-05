<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import {Channel, invoke} from "@tauri-apps/api/core";

  let updateState: UpdateState = $state({ type: "update-available" });

  let {
    onLater,
  }: {
    onLater: () => void,
  } = $props();

  type UpdateEvent =
    { event: "started" } |
    { event: "progress", data: { percent: number } } |
    { event: "restarting" };

  type UpdateState =
    { type: "update-available" } |
    { type: "downloading", percent: number } |
    { type: "restarting" };

  function performUpdateApp() {
    let onEvent = new Channel<UpdateEvent>();
    onEvent.onmessage = (event: UpdateEvent) => {
      if (event.event === "started") {
        updateState = {type: "downloading", percent: 0};
      } else if (event.event === "progress") {
        updateState = {type: "downloading", percent: event.data.percent};
      } else if (event.event === "restarting") {
        updateState = {type: "restarting"};
      }

      console.log("Update state", updateState);
    };
    invoke("perform_update_app", { onEvent }).then((result) => {
      console.log("Update result", result);
    })
  }
</script>

<container role="dialog" aria-live="polite" aria-modal="false">
  <toast>
    {#if updateState.type === "update-available" }
      <div class="message" aria-label="Update available message">
        An update is available.
      </div>
      <div class="actions">
        <button class="cancel" onclick={onLater}>Later</button>
        <button class="confirm" onclick={performUpdateApp}>Update</button>
      </div>
    {:else if updateState.type === "downloading" }
      <div class="message" aria-label="Downloading update">
        Downloading update... {Math.ceil(updateState.percent)}%
      </div>
    {:else if updateState.type === "restarting" }
      <div class="message" aria-label="Restarting message">
        Restarting to apply the update...
      </div>
    {/if}
  </toast>
</container>

<style>
    container {
        position: fixed;
        left: 50%;
        bottom: 1rem;
        transform: translateX(-50%);
        z-index: 1000;
        display: block;
    }

    toast {
        display: flex;
        flex-direction: row;
        align-items: center;
        gap: 0.75rem;
        min-width: 320px;
        max-width: 90vw;
        padding: 0.75rem 1rem;
        border-radius: 8px;
        background: color-mix(in oklab, canvas, black 10%);
        border: 1px solid color-mix(in oklab, canvastext, transparent 85%);
        box-shadow: 0 10px 18px rgba(0,0,0,0.2), 0 2px 6px rgba(0,0,0,0.15);
        outline: none;
    }

    .message {
        flex: 1 1 auto;
        white-space: pre-wrap;
    }

    .actions {
        display: flex;
        gap: 0.5rem;
        justify-content: flex-end;
    }

    button.confirm {
        /* uses global button styles from app.css */
    }
</style>
