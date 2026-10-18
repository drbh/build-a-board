<!-- A "layers" button with a popover of checkboxes that show and hide the
     drawing's layers. -->
<script lang="ts">
  import type { SvelteSet } from "svelte/reactivity";
  import { type Layer, layers } from "../lib/layers";

  let { hidden }: { hidden: SvelteSet<Layer> } = $props();
  let open = $state(false);
  let root: HTMLDivElement;

  function outside(e: PointerEvent) {
    if (open && !root.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window onpointerdown={outside} onkeydown={(e) => e.key === "Escape" && (open = false)} />

<div class="layers" bind:this={root}>
  <button onclick={() => (open = !open)} aria-expanded={open} title="Show and hide layers">
    layers <span class="count">{layers.length - hidden.size}/{layers.length}</span>
  </button>
  {#if open}
    <div class="menu" role="menu">
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
  {/if}
</div>

<style>
  .layers {
    position: relative;
  }
  .count {
    color: var(--faint);
  }
  .menu {
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    z-index: 10;
    display: grid;
    gap: 2px;
    min-width: 150px;
    padding: 6px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    border-radius: 6px;
    box-shadow: 0 8px 24px #0008;
  }
  label {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 3px 6px;
    border-radius: 4px;
    color: var(--muted);
    cursor: pointer;
  }
  label:hover {
    background: var(--control-hover);
    color: var(--ink);
  }
  input {
    margin: 0;
    accent-color: var(--accent);
  }
  i {
    width: 10px;
    height: 10px;
    border-radius: 2px;
    display: inline-block;
  }
</style>
