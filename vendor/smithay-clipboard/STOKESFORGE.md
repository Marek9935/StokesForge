# Wayland drop transition (#72)

Copy of smithay-clipboard 0.7.3 (crates.io), MIT license kept in `LICENSE`.
The clipboard stays available; its receiver also handles the drop, and its
worker is shared among egui's viewports.

Provenance, diff, cause of the first prototype, checks and removal of the two
patches: [`../winit/STOKESFORGE.md`](../winit/STOKESFORGE.md).
