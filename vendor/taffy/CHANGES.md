# Local changes

Based on Taffy 0.10.1 (MIT), as published on crates.io.

Backported the grid changes from upstream commit `631ae42f8bea`
("Include container's own padding in block/grid content_size", #1051).
Grid overflow must include each cell's position relative to the container,
not just its size within that cell. The correction also preserves RTL,
border, padding and absolutely positioned item semantics.

The block-layout changes from that commit are not included.
`tests/grid_scroll_extent.rs` checks multi-track overflow in both directions.
