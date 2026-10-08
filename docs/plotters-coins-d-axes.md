# Rapport amont Plotters — coin vide entre deux axes

À déposer sur <https://github.com/plotters-rs/plotters/issues> (en anglais).

**Title:** Axis lines of a cartesian chart don't meet at the corners (1-pixel gap)

**Version:** plotters 0.3.7, any backend (reproduced with a bitmap/SVG-like backend; here a custom `DrawingBackend`).

**Description**

With `configure_mesh()` on a `ChartBuilder::build_cartesian_2d` chart (label areas on the bottom and left), the X axis line and the Y axis line do not touch: the pixel at the corner is never drawn, leaving a visible 1-pixel notch at the origin. It is more visible when the figure is scaled up. The same happens at the top/right corners with `configure_secondary_axes()`.

**Cause**

`DrawingArea` code in `src/chart/context/cartesian2d/draw_impl.rs`, `draw_axis`: each axis line is drawn in the adjacent label area, from `axis_range.start` to `axis_range.end`, i.e. exactly the pixel extent of the plotting area. The Y axis is therefore at `x = origin.x - 1` and the X axis at `y = origin.y + height`; the corner pixel `(origin.x - 1, origin.y + height)` belongs to neither line. `axis_style` only carries colour and stroke width, so there is no way to close the corner from user code short of drawing an extra element.

**Expected**

Axis lines meet at the corner (e.g. extend the line by one pixel toward the perpendicular axis, or draw it in the corner pixel).

**Minimal repro**

```rust
let mut chart = ChartBuilder::on(&area)
    .x_label_area_size(30)
    .y_label_area_size(30)
    .build_cartesian_2d(0f64..10f64, 0f64..10f64)?;
chart.configure_mesh().draw()?;
// Inspect the pixels around the bottom-left corner of the plotting area.
```
