<!-- The playground: edit a board on the left, see it drawn on the right.

     The worker compiles the board with rustc running in this tab and returns
     its design as JSON; bb-core (compiled to WebAssembly) resolves it against
     KiCad footprints fetched on demand and draws the same SVG `bb preview`
     draws, highlighting what the last edit changed. -->
<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import BoardView from "./components/BoardView.svelte";
  import Editor from "./components/Editor.svelte";
  import Layers from "./components/Layers.svelte";
  import Problems from "./components/Problems.svelte";
  import { examples } from "./lib/examples";
  import { type Layer, hiddenAtStart } from "./lib/layers";
  import { selftest } from "./lib/selftest";
  import { Session } from "./lib/session.svelte";

  let editor: Editor;
  let boardView: BoardView;
  const session = new Session(() => editor.text());
  const hidden = new SvelteSet<Layer>(hiddenAtStart);
  let example = $state(examples.find((e) => e.name === "indicator")?.name ?? examples[0].name);

  function load(name: string) {
    example = name;
    editor.load(examples.find((e) => e.name === name)!.source);
    session.reset();
  }

  onMount(() => {
    if (new URLSearchParams(location.search).has("selftest")) {
      session.onOutcome = selftest({
        text: () => editor.text(),
        replace: (from, to, insert) => editor.replace(from, to, insert),
        compileNow: () => session.compileNow(),
        changes: () => session.changes,
        svgBytes: () => session.svg?.length ?? 0,
      });
    }
    editor.load(examples.find((e) => e.name === example)!.source);
    session.start();
  });
</script>

<header>
  <h1>bb playground</h1>
  <select value={example} onchange={(e) => load(e.currentTarget.value)} aria-label="Example board">
    {#each examples as { name } (name)}
      <option value={name}>{name}</option>
    {/each}
  </select>
  <span id="state" class={session.status.kind}>{session.status.text}</span>
  <span class="changes">{session.changes}</span>
  <span class="spacer"></span>
  <Layers {hidden} />
  <button onclick={() => boardView.fit()} title="Fit the board (F)">Fit</button>
</header>
<main>
  <Editor bind:this={editor} problems={session.problems} onedit={() => session.schedule()} />
  <section>
    <BoardView bind:this={boardView} svg={session.svg} stale={session.stale} {hidden} board={example} />
    <Problems problems={session.problems} onreveal={(line, col) => editor.reveal(line, col)} />
  </section>
</main>

<style>
  header {
    padding: 8px 16px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
    display: flex;
    gap: 6px 14px;
    align-items: center;
    flex-wrap: wrap;
  }
  h1 {
    font-size: 14px;
    margin: 0;
  }
  .spacer {
    flex: 1;
  }
  select,
  button {
    background: var(--line);
    border: 0;
    border-radius: 4px;
    padding: 3px 8px;
    cursor: pointer;
  }
  #state {
    color: var(--muted);
  }
  #state::before {
    content: "● ";
  }
  .busy::before {
    color: var(--warn);
  }
  .ok::before {
    color: var(--ok);
  }
  .bad::before {
    color: var(--bad);
  }
  .changes {
    color: var(--moved);
  }
  main {
    flex: 1;
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    min-height: 0;
  }
  @media (max-width: 900px) {
    main {
      grid-template-columns: 1fr;
      grid-template-rows: 1fr 1fr;
    }
  }
  section {
    display: flex;
    flex-direction: column;
    min-height: 0;
    position: relative;
  }
</style>
