<!-- The compiler's diagnostics and the design's problems; one with a source
     location jumps the editor there. -->
<script lang="ts">
  import type { Problem } from "../lib/protocol";

  let { problems, onreveal }: { problems: Problem[]; onreveal: (line: number, col: number) => void } = $props();
</script>

{#if problems.length}
  <div class="problems">
    {#each problems as problem, i (i)}
      {#if problem.span}
        {@const span = problem.span}
        <button class="problem {problem.level}" onclick={() => onreveal(span.line, span.col)}>{problem.rendered}</button>
      {:else}
        <div class="problem {problem.level}">{problem.rendered}</div>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .problems {
    max-height: 40%;
    overflow: auto;
    border-top: 1px solid var(--line);
    background: var(--panel);
  }
  .problem {
    display: block;
    width: 100%;
    text-align: left;
    background: none;
    border: 0;
    border-bottom: 1px solid var(--line);
    padding: 6px 16px;
    white-space: pre-wrap;
    font: 12px/1.4 ui-monospace, Menlo, monospace;
  }
  .error {
    color: #f2b8b8;
  }
  .warning {
    color: #eadb9c;
  }
  button.problem {
    cursor: pointer;
  }
  button.problem:hover {
    background: #1c232d;
  }
</style>
