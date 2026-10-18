// The drawing's toggleable layers: the class bb-core's SVG gives each
// (`k-<key>`, or `changes` and `findings` for the change highlights and the
// placement checks' marks), a label, and a swatch.
export const layers = [
  { key: "cu-f", name: "F.Cu", color: "--cu-f" },
  { key: "cu-b", name: "B.Cu", color: "--cu-b" },
  { key: "pad", name: "Pads", color: "--tht" },
  { key: "via", name: "Vias", color: "--via" },
  { key: "zone-f", name: "F zones", color: "--cu-f" },
  { key: "zone-b", name: "B zones", color: "--cu-b" },
  { key: "silk-f", name: "Silk", color: "--silk" },
  { key: "fab", name: "Fab", color: "--fab" },
  { key: "rats", name: "Ratsnest", color: "--rats" },
  { key: "changes", name: "Changes", color: "--moved" },
  { key: "findings", name: "Checks", color: "--removed" },
] as const;

export type Layer = (typeof layers)[number]["key"];

// What the last edit changed is on request: unasked, it reads as part of the board.
export const hiddenAtStart: Layer[] = ["fab", "changes"];
