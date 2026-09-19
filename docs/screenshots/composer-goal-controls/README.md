# Native composer controls and questions

Captured from clean source `ce0d10afa70cbe249ac5edae5b819b4621b28cfc` on branch `zeron/composer-workstream-i-would-like-to`, rebased on upstream `657c6842061b39c3eae514bb2148e7c82384f42a`. These PNGs are unmodified native GPUI renders with synthetic content, not mockups or real user conversations.

Build: `CARGO_INCREMENTAL=0 cargo build --locked -p zeron-ui --features appshots-fixture --example composer-polish-fixture`. Build completed at `2026-09-19T20:22:41.722374+00:00`. The dev-profile executable was `target/debug/examples/composer-polish-fixture`, SHA-256 `7afcea37268d8976f7c71810ee782dc87455ad1d7ae58ddc171769db5d7c7bcb`. Launched PIDs were 26945 (frosted), 36064 (opaque), and 36378 (reduced motion); their executable paths were checked against this checkout. Each run exited successfully and produced 125 PNGs. Only selected inspected images are committed here.

The fixture uses an internal temporary settings directory and an in-memory synthetic RPC host, with no engine listener or IPC port. Its strict host checks the chat/device and exact SetGoal actions. Native Enter/Space interactions exercise Pause, Resume, Edit cancellation, Edit save, and Delete, asserting that the ordinary message draft survives every operation. Keyboard and wheel assertions also cover activity, questions and queue. Static status fixtures include active, paused, blocked, complete, usage-limited and budget-limited states. The nine provider selections are synthetic; they do not establish live account behavior.

| Capture | Appearance / material / motion | Actual size | Inspected observation | Result |
| --- | --- | --- | --- | --- |
| [goal-paused.png](goal-paused.png) | Dark / frosted / standard | 840×740 logical; 1680×1480 raster | Pause changes status; Resume, Edit and Delete remain visible; ordinary draft retained. | Pass |
| [goal-active.png](goal-active.png) | Dark / frosted / standard | 840×740 logical; 1680×1480 raster | Resume changes status; Pause replaces Resume; draft retained. | Pass |
| [goal-edited.png](goal-edited.png) | Dark / frosted / standard | 840×740 logical; 1680×1480 raster | Saved objective replaces old text; ordinary draft retained. | Pass |
| [question-dark.png](question-dark.png) | Dark / frosted / standard | 840×816 logical; 1680×1632 raster | Compact choices and cancel control; normal answer pill remains unobstructed. | Pass |
| [question-light-opaque.png](question-light-opaque.png) | Light / opaque / standard | 440×816 logical; 880×1632 raster | Descriptions wrap at narrow width; all three options remain readable. | Pass |
| [goal-light-opaque.png](goal-light-opaque.png) | Light / opaque / standard | 840×740 logical; 1680×1480 raster | Visible goal controls; normal composer placeholder after question scene. | Pass |
| [stack-light-minimum.png](stack-light-minimum.png) | Light / frosted / standard | 440×520 logical; 880×1040 raster | Activity, queue and question share bounded space; answer editor stays reachable. | Pass |
| [stack-dark-reduced-motion.png](stack-dark-reduced-motion.png) | Dark / frosted / reduced | 440×520 logical; 880×1040 raster | Same minimum-size stack with reduced motion; independent wheel assertions pass. | Pass |

Reproduce from the source SHA:

```sh
CARGO_INCREMENTAL=0 cargo build --locked -p zeron-ui --features appshots-fixture --example composer-polish-fixture
ZERON_FIXTURE_SURFACE=frosted target/debug/examples/composer-polish-fixture /tmp/composer-frosted
ZERON_FIXTURE_SURFACE=opaque target/debug/examples/composer-polish-fixture /tmp/composer-opaque
ZERON_FIXTURE_REDUCE_MOTION=1 target/debug/examples/composer-polish-fixture /tmp/composer-reduced
```

The normal requested 840×960 window was clamped by the display to 840×816. Goal scenes use 840×740; minimum stack scenes use 440×520. The table reports actual PNG dimensions, not requested dimensions. Source changes after the captured commit are documentation/evidence only.

The [previous stack evidence](../composer-stacks/README.md) records source `e3961051f127f39938931652eea4ee434f031a08` at the same minimum size. It is retained as the pre-correction reference; the question content and goal controls changed, so this is not a pixel-identical fixture comparison. The user-reported screenshots were inspected privately and are not published.

This evidence establishes rendered layouts, keyboard action dispatch and synthetic RPC round trips. It does not prove animation smoothness, frame time, idle CPU, or live end-to-end UI/account behavior. A separate bounded live Codex 0.154 protocol probe completed an automatically resumed goal in 7.25 seconds; the capability document records that narrower protocol evidence and the other harness limits. Desktop and iOS transcription are excluded.
