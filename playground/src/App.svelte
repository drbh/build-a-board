<!-- The playground: edit a board on the left, see it drawn on the right.

     The worker compiles the board with rustc running in this tab and returns
     its design as JSON; bb-core (compiled to WebAssembly) resolves it against
     KiCad footprints fetched on demand and draws the same SVG `bb preview`
     draws, highlighting what the last edit changed. The address bar is the
     share link (lib/share.ts). -->
<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import BoardView from "./components/BoardView.svelte";
  import Editor from "./components/Editor.svelte";
  import Layers from "./components/Layers.svelte";
  import Problems from "./components/Problems.svelte";
  import Stepper from "./components/Stepper.svelte";
  import { examples } from "./lib/examples";
  import { type Layer, hiddenAtStart } from "./lib/layers";
  import { selftest } from "./lib/selftest";
  import { Session } from "./lib/session.svelte";
  import { readShare, shareQuery } from "./lib/share";
  import { TOUR, stepDiff, steps } from "./lib/tour";

  let editor: Editor;
  let boardView: BoardView;
  let main: HTMLElement;
  const session = new Session(() => editor.text());
  const hidden = new SvelteSet<Layer>(hiddenAtStart);
  /// New visitors start on the tour; the self-test drives the indicator board.
  let example = $state(new URLSearchParams(location.search).has("selftest") ? "indicator" : TOUR);
  /// The tour's current step, from 0.
  let step = $state(0);
  /// On a narrow screen one pane shows at a time.
  let tab = $state<"source" | "board">("source");
  let copied = $state(false);

  /// The editor's share of the width, in percent; remembered per browser.
  let left = $state(stored("bb.split", 50));
  function stored(key: string, fallback: number): number {
    try {
      const v = Number(localStorage.getItem(key));
      return v >= 20 && v <= 80 ? v : fallback;
    } catch {
      return fallback;
    }
  }

  const original = () =>
    example === TOUR ? steps[step].source : examples.find((e) => e.name === example)!.source;
  const urlState = () => ({
    example,
    step: example === TOUR ? step : undefined,
    source: editor.text(),
    original: original(),
  });

  function load(name: string, source?: string) {
    example = name;
    step = 0;
    editor.load(source ?? original());
    session.reset();
    updateUrl();
  }

  /// Move through the tour. The editor marks the lines this step changed; the
  /// renderer keeps the last drawing, so the Changes layer shows what it drew.
  function go(to: number) {
    step = Math.min(Math.max(to, 0), steps.length - 1);
    editor.load(original(), stepDiff(step));
    session.compileNow();
    updateUrl();
  }

  let urlTimer: ReturnType<typeof setTimeout> | undefined;
  function updateUrl() {
    clearTimeout(urlTimer);
    urlTimer = setTimeout(async () => {
      const query = await shareQuery(location.search, urlState());
      history.replaceState(null, "", query);
    }, 300);
  }

  async function share() {
    clearTimeout(urlTimer);
    history.replaceState(null, "", await shareQuery(location.search, urlState()));
    try {
      await navigator.clipboard.writeText(location.href);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      // Clipboard access denied: the address bar already holds the link.
    }
  }

  function drag(e: PointerEvent) {
    e.preventDefault();
    const rect = main.getBoundingClientRect();
    const move = (m: PointerEvent) => {
      left = Math.min(80, Math.max(20, ((m.clientX - rect.left) / rect.width) * 100));
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      try {
        localStorage.setItem("bb.split", String(Math.round(left)));
      } catch {
        // Storage unavailable: the split just isn't remembered.
      }
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  }

  onMount(async () => {
    if (new URLSearchParams(location.search).has("selftest")) {
      session.onOutcome = selftest({
        text: () => editor.text(),
        replace: (from, to, insert) => editor.replace(from, to, insert),
        compileNow: () => session.compileNow(),
        changes: () => session.changes,
        svgBytes: () => session.svg?.length ?? 0,
      });
    }
    const shared = await readShare(location.search);
    if (shared.example === TOUR || examples.some((e) => e.name === shared.example)) example = shared.example!;
    if (example === TOUR && shared.step !== null) step = Math.min(shared.step, steps.length - 1);
    editor.load(shared.source ?? original(), example === TOUR && !shared.source ? stepDiff(step) : null);
    session.start();
  });
</script>

<div class="app">
  <header class="top">
    <span class="brand">bb</span>
    <span class="sub">playground</span>
    <nav class="tabs" aria-label="Pane">
      <button class:on={tab === "source"} onclick={() => (tab = "source")}>source</button>
      <button class:on={tab === "board"} onclick={() => (tab = "board")}>board</button>
    </nav>
    <span class="spacer"></span>
    <button onclick={share} title="Copy a link to this board, edits included">{copied ? "copied ✓" : "share"}</button>
    <a href="book/">book</a>
    <a class="github" href="https://github.com/drbh/build-a-board">github</a>
  </header>
  <main bind:this={main} style:--left="{left}%" class="show-{tab}">
    <section class="pane source">
      <div class="pane-head">
        <span class="label">lib.rs</span>
        <select value={example} onchange={(e) => load(e.currentTarget.value)} aria-label="Example board">
          <option value={TOUR}>tour: build a board</option>
          {#each examples as { name } (name)}
            <option value={name}>{name}</option>
          {/each}
        </select>
        <span class="spacer"></span>
        <span class="hint" title="Rebuilds on its own as you type">⌘⏎ rebuild</span>
      </div>
      {#if example === TOUR}
        <Stepper {step} ongo={go} />
      {/if}
      <Editor
        bind:this={editor}
        problems={session.problems}
        onedit={() => (session.schedule(), updateUrl())}
        onrun={() => session.compileNow()}
      />
      <Problems
        problems={session.problems}
        status={session.status}
        onreveal={(line, col) => ((tab = "source"), editor.reveal(line, col))}
      />
    </section>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="split" onpointerdown={drag} title="Drag to resize"></div>
    <section class="pane board">
      <div class="pane-head">
        <span class="label">board</span>
        {#if !hidden.has("changes")}
          <span class="changes" title={session.changes}>{session.changes}</span>
        {/if}
        <span class="spacer"></span>
        <Layers {hidden} />
        <button onclick={() => boardView.fit()} title="Fit the board (F)">fit</button>
      </div>
      <BoardView bind:this={boardView} svg={session.svg} stale={session.stale} {hidden} board={example} />
    </section>
  </main>
</div>

<style>
  .app {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
    padding: 6px 12px;
    background: var(--app-bg);
    border-bottom: 1px solid var(--line);
  }
  .brand {
    font-weight: 700;
  }
  .sub {
    color: var(--faint);
    margin-left: -6px;
  }
  .spacer {
    flex: 1;
  }
  .tabs {
    display: none;
    gap: 4px;
  }
  .tabs button.on {
    border-color: var(--accent);
  }
  main {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, var(--left)) 5px minmax(0, 1fr);
    background: var(--workspace-bg);
  }
  .pane {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    position: relative;
  }
  .split {
    background: var(--line-light);
    border-left: 2px solid var(--workspace-bg);
    border-right: 2px solid var(--workspace-bg);
    cursor: col-resize;
  }
  .split:hover {
    background: var(--accent);
  }
  .hint {
    color: var(--faint);
    font-size: 12px;
  }
  .changes {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--moved);
    font-size: 12px;
  }
  @media (max-width: 760px) {
    .tabs {
      display: flex;
    }
    main {
      grid-template-columns: minmax(0, 1fr);
    }
    .split,
    .hint,
    .top .sub,
    .github,
    main.show-source .board,
    main.show-board .source {
      display: none;
    }
  }
</style>
