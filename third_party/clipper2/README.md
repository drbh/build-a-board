# Clipper2 1.3.0 (as vendored by KiCad 10.0.5)

Verbatim copy of `thirdparty/clipper2/Clipper2Lib` from the KiCad 10.0.5 source
tree (https://gitlab.com/kicad/code/kicad/-/tree/10.0.5/thirdparty/clipper2),
Boost Software License 1.0 (see `LICENSE`). It is compiled with `USINGZ`, as
KiCad compiles it, so polygon unions match pcbnew exactly.

`shim.cpp` is ours: a minimal C ABI over the one operation `bb` needs, the
NonZero union KiCad's `SHAPE_POLY_SET::Simplify()` performs.
