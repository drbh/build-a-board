// Messages between the page and the compiler worker.

/// A rustc diagnostic, or a design or runtime failure (which has no span).
export interface Problem {
  level: "error" | "warning";
  message: string;
  rendered: string;
  span?: { line: number; col: number; endLine: number; endCol: number };
}

export type Request = { type: "init"; base: string } | { type: "compile"; id: number; source: string };

export interface Compiled {
  type: "compiled";
  id: number;
  compileMs?: number;
  diagnostics?: Problem[];
  /// Whatever rustc printed that was not a diagnostic.
  residue?: string;
  /// The board's design, when it compiled and ran.
  json?: string;
  /// The board's panic message, when it compiled but `build()` panicked.
  panic?: string;
  /// The worker itself failed.
  engineError?: string;
}

export type Reply = { type: "ready"; ms: number } | Compiled;
