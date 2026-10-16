<!-- Checkboxes that show and hide the drawing's layers. -->
<script lang="ts">
  import type { SvelteSet } from "svelte/reactivity";
  import { type Layer, layers } from "../lib/layers";

  let { hidden }: { hidden: SvelteSet<Layer> } = $props();
</script>

<div class="layers">
  {#each layers as layer (layer.key)}
    <label>
      <input
        type="checkbox"
        checked={!hidden.has(layer.key)}
        onchange={(e) => (e.currentTarget.checked ? hidden.delete(layer.key) : hidden.add(layer.key))}
      />
      <i style:background="var({layer.color})"></i>{layer.name}
    </label>
  {/each}
</div>

<style>
  .layers {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 10px;
  }
  label {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--muted);
    cursor: pointer;
  }
  i {
    width: 10px;
    height: 10px;
    border-radius: 2px;
    display: inline-block;
  }
</style>
