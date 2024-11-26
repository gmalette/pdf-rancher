<script lang="ts">
  import {listen} from "@tauri-apps/api/event";

  let errors: string[] = $state([])

  function removeError(index: number) {
    errors = errors.filter((_, i) => i !== index)
  }

  listen("rancher://error", (e) => {
    errors = [...errors, e.payload as string]
  })
</script>

{#if errors.length > 0}
  <banners>
    {#each errors as error, i}
      <banner class="error">
        <p>{error}</p>
        <a onclick={() => removeError(i)}>
          <i class="fas fa-times close" ></i>
        </a>
      </banner>
    {/each}
  </banners>
{/if}

<style>
    banners {
        display: block;
        padding-bottom: 1rem;
    }

    banner {
        display: flex;
        width: 100%;
        height: auto;
        padding: 10px;
        border-radius: 5px;
        font-weight: 400;
        margin-bottom: 0.5rem;

        &.error {
            background-color: #FEE;
            border: 1px solid #EDD;
            color: #A66;
        }

        p {
            margin: 0;
            display: flex;
            flex-grow: 1;
        }

        i.close {
            cursor: pointer;
        }
    }
</style>
