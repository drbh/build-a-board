<!-- The board drawing: pan by dragging, zoom with the wheel, click a pad or
     track to highlight its net (Escape clears it), F fits the board. -->
<script lang="ts">
  import type { SvelteSet } from "svelte/reactivity";
  import type { Layer } from "../lib/layers";
  import "../lib/drawing.css";

  let {
    svg,
    stale = false,
    hidden,
    board,
  }: {
    svg: string | null;
    stale?: boolean;
    hidden: SvelteSet<Layer>;
    /// Which board this is: a new one starts at its own home view.
    board: string;
  } = $props();

  type Box = [number, number, number, number];

  let container: HTMLDivElement;
  /// The view the reader panned or zoomed to, kept across redraws.
  let view: Box | null = null;
  let net: string | null = $state(null);
  let drag: { x: number; y: number; view: Box; moved: boolean } | null = null;
  let dragging = $state(false);

  const element = () => container?.querySelector<SVGSVGElement>(":scope > svg") ?? null;
  const viewBox = (el: SVGSVGElement) => el.getAttribute("viewBox")!.split(" ").map(Number) as Box;
  function setView(box: Box) {
    element()?.setAttribute("viewBox", box.join(" "));
    view = box;
  }

  $effect(() => {
    board;
    view = null;
    net = null;
  });

  // After each redraw: restore the view and the highlighted net.
  $effect(() => {
    svg;
    const el = element();
    if (!el) return;
    if (view) el.setAttribute("viewBox", view.join(" "));
    el.classList.toggle("highlight", !!net);
    for (const part of el.querySelectorAll<SVGElement>("[data-net]")) {
      part.classList.toggle("hl", !!net && part.dataset.net === net);
    }
  });

  export function fit() {
    const el = element();
    if (el?.dataset.home) el.setAttribute("viewBox", el.dataset.home);
    view = null;
  }

  /// Screen pixels per board unit, and where the drawing starts on screen.
  function scale(el: SVGSVGElement, [, , w, h]: Box) {
    const r = el.getBoundingClientRect();
    const s = Math.min(r.width / w, r.height / h);
    return { s, left: r.left + (r.width - w * s) / 2, top: r.top + (r.height - h * s) / 2 };
  }

  function wheel(e: WheelEvent) {
    const el = element();
    if (!el) return;
    e.preventDefault();
    const box = viewBox(el);
    const [x, y, w, h] = box;
    const { s, left, top } = scale(el, box);
    const px = x + (e.clientX - left) / s;
    const py = y + (e.clientY - top) / s;
    const k = Math.exp(e.deltaY * 0.0015);
    setView([px - (px - x) * k, py - (py - y) * k, w * k, h * k]);
  }

  function pointerdown(e: PointerEvent) {
    const el = element();
    if (el) drag = { x: e.clientX, y: e.clientY, view: viewBox(el), moved: false };
  }

  function pointermove(e: PointerEvent) {
    const el = element();
    if (!drag || !el) return;
    const [x, y, w, h] = drag.view;
    const { s } = scale(el, drag.view);
    const dx = e.clientX - drag.x;
    const dy = e.clientY - drag.y;
    if (Math.abs(dx) + Math.abs(dy) > 3) drag.moved = dragging = true;
    if (drag.moved) setView([x - dx / s, y - dy / s, w, h]);
  }

  function pointerup(e: PointerEvent) {
    if (drag && !drag.moved) {
      net = (e.target as Element).closest<SVGElement>("[data-net]")?.dataset.net ?? null;
    }
    drag = null;
    dragging = false;
  }

  function keydown(e: KeyboardEvent) {
    if ((e.target as Element).closest?.(".cm-editor, input, select")) return;
    if (e.key === "Escape") net = null;
    if (e.key === "f" || e.key === "F") fit();
  }
</script>

<svelte:window onkeydown={keydown} />

<div
  class="drawing {[...hidden].map((k) => `hide-${k}`).join(' ')}"
  class:stale
  class:dragging
  role="img"
  aria-label="Board drawing"
  bind:this={container}
  onwheel={wheel}
  onpointerdown={pointerdown}
  onpointermove={pointermove}
  onpointerup={pointerup}
  onpointerleave={() => ((drag = null), (dragging = false))}
>
  {#if svg}
    {@html svg}
  {:else}
    <p class="hint">
      The board appears here once it compiles. It is compiled by rustc running in this tab, about half a second
      after you stop typing.
    </p>
  {/if}
</div>
{#if net}<div class="net">net {net}</div>{/if}

<!-- Markers and hatches the drawing refers to by id. -->
<svg width="0" height="0" style="position:absolute" aria-hidden="true">
  <defs>
    <marker
      id="arrow"
      viewBox="0 0 10 10"
      refX="9"
      refY="5"
      markerWidth="5"
      markerHeight="5"
      orient="auto-start-reverse"
    >
      <path d="M0,0 L10,5 L0,10 z" fill="#ff4fd8" />
    </marker>
    <pattern id="hatch-f" width="1" height="1" patternUnits="userSpaceOnUse" patternTransform="rotate(45)">
      <line x1="0" y1="0" x2="0" y2="1" stroke="#c94a4a" stroke-width=".12" opacity=".5" />
    </pattern>
    <pattern id="hatch-b" width="1" height="1" patternUnits="userSpaceOnUse" patternTransform="rotate(-45)">
      <line x1="0" y1="0" x2="0" y2="1" stroke="#4a7ccf" stroke-width=".12" opacity=".45" />
    </pattern>
  </defs>
</svg>

<style>
  .drawing {
    flex: 1;
    min-height: 0;
    position: relative;
  }
  .drawing > :global(svg) {
    width: 100%;
    height: 100%;
    display: block;
    cursor: grab;
  }
  .dragging > :global(svg) {
    cursor: grabbing;
  }
  .stale > :global(svg) {
    opacity: 0.55;
  }
  .hint {
    padding: 12px 16px;
    margin: 0;
    color: var(--muted);
  }
  .net {
    position: absolute;
    top: 8px;
    right: 12px;
    background: var(--panel);
    padding: 2px 8px;
    border-radius: 4px;
  }
</style>
