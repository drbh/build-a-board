<!-- The tour's stepper: back and forward through the steps, with the current
     step's title and what to notice in it. -->
<script lang="ts">
  import { steps } from "../lib/tour";

  let { step, ongo }: { step: number; ongo: (step: number) => void } = $props();
  const current = $derived(steps[step]);
</script>

<div class="stepper">
  <div class="row">
    <button onclick={() => ongo(step - 1)} disabled={step === 0} aria-label="previous step">◂</button>
    <span class="at">{step + 1}/{steps.length}</span>
    <button onclick={() => ongo(step + 1)} disabled={step === steps.length - 1} aria-label="next step">▸</button>
    <span class="title">{current.title}</span>
    {#if step > 0}<span class="legend"><i></i>changed since step {step}</span>{/if}
  </div>
  <p class="note">{current.note}</p>
  <div class="dots" aria-hidden="true">
    {#each steps as s, i (i)}
      <button class="dot" class:on={i === step} class:done={i < step} title={s.title} onclick={() => ongo(i)}></button>
    {/each}
  </div>
</div>

<style>
  .stepper {
    padding: 8px 10px 10px;
    background: var(--panel);
    border-bottom: 1px solid var(--line-light);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .at {
    color: var(--muted);
    min-width: 3ch;
    text-align: center;
  }
  .title {
    font-weight: 700;
    margin-left: 4px;
  }
  .legend {
    margin-left: auto;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--faint);
    font-size: 12px;
    white-space: nowrap;
  }
  .legend i {
    width: 10px;
    height: 10px;
    background: #69b78030;
    box-shadow: inset 2px 0 var(--ok);
  }
  .note {
    margin: 6px 0 8px;
    color: var(--muted);
    line-height: 1.5;
  }
  .dots {
    display: flex;
    gap: 4px;
  }
  .dot {
    flex: 1;
    height: 4px;
    padding: 0;
    border: 0;
    border-radius: 2px;
    background: var(--line);
  }
  .dot.done {
    background: var(--faint);
  }
  .dot.on {
    background: var(--accent);
  }
</style>
