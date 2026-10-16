// The wasm-pack build of core-wasm (tools/assets.sh writes it to .cache/core).

export interface Drawing {
  svg: string;
  /// What changed since the last drawing, e.g. "moved J1 by (+3.00, +0.00)".
  changes: string | null;
  status: string;
  warnings: string[];
  parts: number;
  unrouted: number;
}

export class Renderer {
  constructor();
  free(): void;
  /// The `Library:Name` footprints the design places that have not been added yet.
  missing_footprints(json: string): string[];
  add_footprint(id: string, text: string): void;
  render(json: string): Drawing;
}

export default function init(): Promise<unknown>;
