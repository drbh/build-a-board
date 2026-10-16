// One editing session: compile the editor's text in the worker, resolve and
// draw its design with bb-core, and keep the last good drawing on screen
// (dimmed) while the board does not build.
import initCore, { type Drawing, Renderer } from "$core";
import CompilerWorker from "./compiler.worker?worker";
import type { Compiled, Problem, Reply, Request } from "./protocol";

export type Kind = "busy" | "ok" | "bad";

/// What one compile came to, for the status line and the self-test.
export interface Outcome {
  kind: Kind;
  text: string;
  problems: Problem[];
  drawing?: Drawing;
  wallMs: number;
  compileMs?: number;
}

const ms = (v: number) => `${Math.round(v)} ms`;
const failure = (text: string): Problem => ({ level: "error", message: text, rendered: text });
const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

export class Session {
  status = $state<{ kind: Kind; text: string }>({ kind: "busy", text: "loading the compiler…" });
  /// What the last drawing changed, e.g. "moved J1 by (+3.00, +0.00)".
  changes = $state("");
  svg = $state<string | null>(null);
  /// The drawing is from an earlier, good version of the board.
  stale = $state(false);
  problems = $state<Problem[]>([]);

  /// Called with each compile's outcome (the self-test drives edits from it).
  onOutcome: ((outcome: Outcome) => void | Promise<void>) | null = null;

  #source: () => string;
  #worker: Worker;
  #renderer: Renderer | null = null;
  #ready = false;
  #running = false;
  #pending = false;
  #timer: ReturnType<typeof setTimeout> | undefined;
  #started = 0;
  #id = 0;

  constructor(source: () => string) {
    this.#source = source;
    this.#worker = new CompilerWorker();
    this.#worker.onmessage = ({ data }: MessageEvent<Reply>) => this.#receive(data);
  }

  /// Load bb-core and the compiler; the first compile follows.
  async start() {
    await initCore();
    this.#renderer = new Renderer();
    this.#post({ type: "init", base: new URL("compiler/", document.baseURI).href });
  }

  /// A new board: forget the last drawing's highlights and compile right away.
  reset() {
    if (this.#renderer) {
      this.#renderer.free();
      this.#renderer = new Renderer();
    }
    this.changes = "";
    this.compileNow();
  }

  /// Compile once typing pauses.
  schedule(delay = 400) {
    clearTimeout(this.#timer);
    this.#timer = setTimeout(() => this.#compile(), delay);
  }

  compileNow() {
    clearTimeout(this.#timer);
    this.#compile();
  }

  #post(request: Request) {
    this.#worker.postMessage(request);
  }

  #compile() {
    if (!this.#ready) return;
    if (this.#running) {
      this.#pending = true;
      return;
    }
    this.#running = true;
    this.#pending = false;
    this.#started = performance.now();
    this.status = { kind: "busy", text: "compiling…" };
    this.#post({ type: "compile", id: ++this.#id, source: this.#source() });
  }

  async #receive(reply: Reply) {
    if (reply.type === "ready") {
      this.#ready = true;
      this.status = { kind: "ok", text: `compiler ready in ${ms(reply.ms)}` };
      this.#compile();
      return;
    }
    const outcome = await this.#finish(reply);
    this.#running = false;
    await this.onOutcome?.(outcome);
    if (this.#pending) this.#compile();
  }

  /// Fetch the footprints a design places that the renderer has not seen yet.
  async #fetchFootprints(renderer: Renderer, json: string) {
    await Promise.all(
      renderer.missing_footprints(json).map(async (fp) => {
        const [library, name] = fp.split(":");
        const url = `footprints/${encodeURIComponent(library)}.pretty/${encodeURIComponent(name)}.kicad_mod`;
        const response = await fetch(url);
        if (!response.ok) throw new Error(`footprint ${fp} is not in the KiCad library (${response.status})`);
        renderer.add_footprint(fp, await response.text());
      }),
    );
  }

  async #finish(result: Compiled): Promise<Outcome> {
    const elapsed = () => performance.now() - this.#started;
    const problems = [...(result.diagnostics ?? [])];
    let outcome: Pick<Outcome, "kind" | "text" | "drawing">;
    if (result.engineError !== undefined) {
      problems.push(failure(`compiler error:\n${result.engineError}`));
      outcome = { kind: "bad", text: "compiler error" };
    } else if (result.panic !== undefined) {
      problems.push(failure(`the board panicked:\n${result.panic}`));
      outcome = { kind: "bad", text: "build() panicked" };
    } else if (result.json === undefined) {
      // Leftover output (an internal compiler error, say) only matters without
      // real diagnostics; otherwise it is just rustc's exit trap.
      if (result.residue && !problems.some((p) => p.level === "error")) problems.push(failure(result.residue));
      const errors = problems.filter((p) => p.level === "error").length;
      outcome = { kind: "bad", text: `${plural(errors, "compile error")} · ${ms(elapsed())}` };
    } else {
      try {
        const renderer = this.#renderer!;
        await this.#fetchFootprints(renderer, result.json);
        const drawing = renderer.render(result.json);
        this.svg = drawing.svg;
        this.changes = drawing.changes ?? "";
        for (const w of drawing.warnings) problems.push({ level: "warning", message: w, rendered: `warning: ${w}` });
        const text = `${plural(drawing.parts, "part")} · ${drawing.unrouted} unrouted · ${ms(elapsed())}`;
        outcome = { kind: "ok", text, drawing };
      } catch (e: any) {
        problems.push(failure(String(e?.message ?? e)));
        outcome = { kind: "bad", text: "design problems" };
      }
    }
    this.stale = outcome.kind !== "ok" && this.svg !== null;
    this.status = { kind: outcome.kind, text: outcome.text };
    this.problems = problems;
    return { ...outcome, problems, wallMs: elapsed(), compileMs: result.compileMs };
  }
}
