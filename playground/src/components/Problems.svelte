<!-- The bottom panel: the build's status, and the compiler's diagnostics and
     the design's problems; one with a source location jumps the editor there.
     Collapsed, its header line is the only place a failure shows, so it peeks
     the first problem. -->
<script lang="ts">
  import type { Problem } from "../lib/protocol";
  import type { Kind } from "../lib/session.svelte";

  let {
    problems,
    status,
    onreveal,
  }: {
    problems: Problem[];
    status: { kind: Kind; text: string };
    onreveal: (line: number, col: number) => void;
  } = $props();

  let open = $state(false);
  const errors = $derived(problems.filter((p) => p.level === "error").length);
  const warnings = $derived(problems.length - errors);
  const first = $derived(problems[0]?.message.split("\n")[0] ?? "");
</script>

<div class="panel" class:open>
  <button class="head" onclick={() => (open = !open)} aria-expanded={open}>
    <span class="twisty" class:open>▸</span>
    <span class="label">problems</span>
    {#if errors}<span class="sub bad">{errors} error{errors === 1 ? "" : "s"}</span>{/if}
    {#if warnings}<span class="sub warn">{warnings} warning{warnings === 1 ? "" : "s"}</span>{/if}
    {#if !open && first}
      <span class="peek" class:bad={errors > 0}>{first}</span>
    {:else}
      <span class="spacer"></span>
    {/if}
    <span id="state" class={status.kind}>{status.text}</span>
  </button>
  {#if open}
    <div class="list">
      {#each problems as problem, i (i)}
        {#if problem.span}
          {@const span = problem.span}
          <button class="problem {problem.level}" onclick={() => onreveal(span.line, span.col)}>
            <span class="at">{span.line}:{span.col}</span>{problem.rendered}
          </button>
        {:else}
          <div class="problem {problem.level}">{problem.rendered}</div>
        {/if}
      {:else}
        <p class="empty">
          No problems. Compiler errors, design problems (an unknown pad, a missing footprint) and panics show up here;
          click one with a location to jump to it.
        </p>
      {/each}
    </div>
  {/if}
</div>

<style>
  .panel {
    border-top: 1px solid var(--line-light);
    background: var(--panel);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .panel.open {
    max-height: 40%;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    min-height: 30px;
    padding: 4px 10px;
    background: var(--toolbar);
    border: 0;
    border-radius: 0;
    text-align: left;
    white-space: nowrap;
  }
  .twisty {
    color: var(--faint);
    font-size: 10px;
    transition: transform 0.1s;
  }
  .twisty.open {
    transform: rotate(90deg);
  }
  .sub {
    font-size: 12px;
  }
  .peek {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    color: var(--muted);
  }
  .spacer {
    flex: 1;
  }
  .bad {
    color: var(--bad);
  }
  .warn {
    color: var(--warn);
  }
  #state {
    color: var(--muted);
    font-size: 12px;
  }
  #state::before {
    content: "● ";
  }
  #state.busy::before {
    color: var(--warn);
  }
  #state.ok::before {
    color: var(--ok);
  }
  #state.bad::before {
    color: var(--bad);
  }
  .list {
    overflow: auto;
    min-height: 0;
  }
  .problem {
    display: block;
    width: 100%;
    text-align: left;
    background: none;
    border: 0;
    border-bottom: 1px solid var(--line-light);
    border-radius: 0;
    padding: 6px 12px;
    white-space: pre-wrap;
    font-size: 12px;
    line-height: 1.4;
  }
  .at {
    color: var(--faint);
    margin-right: 10px;
  }
  .error {
    color: #e9b3a3;
  }
  .warning {
    color: #e6d3a6;
  }
  button.problem:hover {
    background: var(--control-hover);
  }
  .empty {
    margin: 0;
    padding: 10px 12px;
    color: var(--faint);
  }
</style>
