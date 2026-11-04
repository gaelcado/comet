# Ask: a script's drag hands off to the platform's, and layout moves keep their speed

**Who needs it:** the session tabs and the panel's tabs (`ui/ds/tab_strip.luau`), seen on zgx `a7146971` on macOS, Nov 5 2026. They reorder as Safari's and Arc's do: the tab lifted follows the pointer along the strip, held to its line, and carried off the strip it becomes a drag the sidebar, a split's panes or another window can take.

**What fails today:**

1. **No hand-off from a gesture to a native drag.** zgx-components' own `tabs.rs` does this in Rust (an in-strip pointer gesture, then `EventContext::start_drag_from` once it's torn off), but Luau has only `on_drag`, which starts the native drag at the drag threshold. So ds/tab_strip starts the native drag at once and follows the pointer through its `on_drag_over` over the strip, hiding the preview (opacity 0) while it's there and drawing the lifted tab itself. It works, but:
   - the strip's hold on the drag is exactly its box: there's no tear-off distance (Safari's few points of give under the strip), since a bigger drop zone would lie over the page and take its presses;
   - the preview pops in at full opacity as the pointer leaves (a drag's preview has no clock);
   - a preview that is invisible but present still needs `layer_revision` to redraw its window when it's shown (it does, `scene.rs`' popup tests), which Zeron relies on.
2. **Layout moves restart at rest.** `ui.each`'s `animate` (FLIP, `motion.rs` `start`) begins each move from where the row shows, which is right, but with no speed: a neighbour that has to turn back mid-slide (a tab dragged back and forth) stops dead, then sets off again. `ui.spring` keeps its velocity (`anim.rs`), so the chosen tab's wash and the tab under it can part for a moment.
3. (Smaller) `layout_id`'s shared-element move scales its node without correcting its corners, so a pill gliding between tabs of different widths stretches its radius. ds/tab_strip draws the wash as one pill with `x` and `w` springs instead, as ds/tabs does.

**The ask:**

1. A Luau way to start the platform's drag from a press already held, at the press's point: `ui.start_drag(payload, { grab_x, grab_y })` from `on_pointer_move`, or a source option that hands a script's own gesture over to the native drag once it leaves a region (`drag_after = function(e) return e.local_y < -12 or e.local_y > h + 12 end`). The tab then follows the pointer through `on_pointer_move` (with `createDraggable`'s axis and container) until it's torn off.
2. FLIP moves that start with the speed the node is moving at, when one retargets a move in flight.

**Today's workaround:** the native drag from the first move, as above. The tab follows the pointer 1:1 along the strip through the drag's own over-events; it lets go of the strip at the strip's edge.
