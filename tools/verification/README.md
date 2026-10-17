# Gameplay journeys

`run-verification` runs journeys in one game process to produce screenshots for
visual review. Journeys are not test gates; fixes are accepted by playing the
scenario in the game. It groups
shell work, Freeform work and Challenge work. A navigation failure stops the batch
and marks the remaining journeys NOT RUN. The summary records fallback map reloads.

Headless runs disable window, audio output and hardware gamepad backends. Scripted mouse,
keyboard and controller events still use Bevy's input processing, so host input
cannot interfere with a journey.

For accelerated manual play on a private display, set
`OPENZT2_SIMULATION_SPEED_MULTIPLIER=5` when launching the game. This scales
simulation time, including fixed updates, while retaining normal pause controls.
It defaults to 1 and does not change saved clock or speed settings. Use the
runner's no-device `ALSA_CONFIG_PATH` configuration for silent private runs.

A journey owns a complete player interaction, including its cleanup. Place,
inspect, move and sell one building in one journey. Reuse the same enclosure for
compatible animal checks. Put irreversible work last in its map: populated save/load and habitat work
after reversible Freeform checks, and research after Challenge sale checks.

`restore freeform-map` and `restore challenge-map` dismiss remaining UI through
ordinary controls and check the original camera, pause and world-count baseline.
They do not assign saved state. A journey must undo its own construction first,
for example with `key CTRL+Z`. Failed restoration fails that journey.

Journeys contain their acceptance checks directly. There are no step files or
includes. `start` uses the runner's ordinary UI navigation; `return-to-menu`
uses the same navigation to discard a world without saving.

- `until ... within N` waits for an observable outcome, bounded by N frames.
- The whole game has one wall-time budget (`--budget`, default 300 seconds).
  When it runs out the running journey fails and later journeys are NOT RUN.
  A journey whose frame rate stays below 10 per second for ten seconds outside
  world loading fails, and the batch moves on. A timeout is a failure, never a pass.
- `expect construction_payments == @before-placement+1` checks exactly one new
  payment even when earlier journeys have already made purchases.
- `hold W until ... within N` releases the key on success, failure or timeout.
- Adjacent numeric `expect` commands inspect one collected observation. An action,
  wait, snapshot or capture separates observations. Keep a delayed check when the
  behaviour being tested is the absence of a duplicate or delayed effect.
- Captures are visual evidence to review, not proof supplied by the assertions.
  Full capture remains the default; both modes bound readback and fact completion.

The default batch excludes diagnostics. Select these explicitly:

```sh
tools/verification/run-verification font-cache-revisit
tools/verification/run-verification capture-readiness
tools/verification/run-verification --profiling frame-time-in-map
```

Font-cache repeats its visit set deliberately to measure warmed allocation growth.
Capture-readiness compares immediate and settled images. Frame-time runs alone;
its threshold must be assessed against the machine and resources used.

Save is first exercised through Load, then through Game Options on the same map.
Both routes check pause ownership and cancellation; a separate cold options map
is not part of acceptance.
Loader error exclusions are checked against the whole game log, including
navigation, regardless of which journey discovers them.

## World visits

The default successful batch has five world loads in one game process:

| Load | Work before leaving |
| --- | --- |
| 1. Large Alpine, Freeform | Loading screen, Save routes, F1, reversible gameplay, staff and save |
| 2. Reload that save | Restored staff/name/admissions, habitat, animals, keeper gates, Return to Menu |
| 3. Small Alpine, Freeform | Another map after returning, walking bounds and camera checks |
| 4. Large Alpine, Challenge | Sale/refund while paused, then the instant challenge offer and research |
| 5. Tutorial 3 | Campaign selection and tutorial launch |

Shell and filter checks load no worlds. Both ordinary launch routes select the
first Alpine row while the Alpine filter is active, preserving the fixture used
by coordinate-based checks. Benthic and expansion list filtering are checked at
selection; this batch no longer separately loads a Benthic world. Use `--maps`
for the catalogue-wide loading sweep.

This is the successful-path count. Failed cleanup can add a fallback reload;
the runner reports those separately. Diagnostics and `--maps` are separate runs.

## Coverage and cleanup

The normal batch uses one process. Journeys starting in the same map reuse it;
`restore` checks their cleanup before the next journey. The following table is
also the ownership map for checks combined from older journeys.

| Journey | Checks it owns | Cleanup or reason for another map |
| --- | --- | --- |
| shell-menu | Splash, main-menu F1 scope, definition errors | Remains on menu |
| campaign-scenario-list | Campaign rows and selected tutorial | Distinct tutorial; returns to menu |
| controller-menu-confirm-cancel | Focused Confirm, modal Cancel, no duplicate actions | Disconnects scripted controller |
| load-save-popup | Empty load selection, Cancel, reopen, Escape | Closes dialog |
| map-biome-filter | Globe, biome/expansion filters, map-selection F1 scope | Cancels selection without loading; navigation later launches filtered Alpine in both modes |
| mod-overlay-close | Archive state, close/reopen, menu reconstruction | Closes overlay |
| options-apply-dismiss | Apply, draft cancellation, hover visibility | Reapplies original settings |
| profile-and-options-lists | Profile rows/name, Escape, close, Continue | Closes dialog |
| first-person-map-bounds | Second map after return, walking bounds, wheel limits/reversal, camera restoration | Small Alpine fixture; returns to menu |
| save-dialog-cancel | First Save projection, both Save routes, pause changes, options/controller, gameplay F1 scope | First journey on the shared Freeform map; restores it |
| pause-menu-hover | Highlight alignment, rapid sweeps, stationary hover and re-entry during a fade | Closes Game Options; restores the shared Freeform map |
| adoption-first-row | First/repeated selection, genders, food/enrichment preview replacement | Cancels previews |
| building-lifecycle | Building panel, relocation/cancel, identifying sale/refund | Sells the created building |
| fixed-function-reflection | Restroom reflection after camera changes | Inverse camera input and construction undo |
| guest-amenities | Food purchase, delivery, eating and hunger | Undoes stand; natural guest state continues |
| guest-gifts | Gift purchase and worn item, no duplicate charge | Undoes cart; natural guest state continues |
| guest-mode-shortcut | G entry, modal/editor suppression, Escape/Overhead | Restores camera and mode |
| minimize-toolbar | Minimize/Maximize without unintended construction | Restores toolbar, dismisses construction |
| overhead-view | Mode rejection, photo zoom/clamp, Overhead/Escape | Restores camera and mode |
| placement-rules | Open-ground versus road validity | Cancels preview |
| purchase-panels | Recommendations, animal/building details, close/reselection | Closes panels and cancels preview |
| remembered-catalogue-selection | Wheel reachability, per-tab selection, portraits | Cancels preview |
| selection-panels | Tree/entrance panel ownership, goals without a challenge | Closes panels |
| object-sale | Budgeted tree sale, Cancel/Escape, one refund | Undoes sale and checks restored budget |
| catalogue-research | Instant challenge offer/accept and goal, paused research purchase, unlock | Discards researched world |
| populated-zoo | Staff looks, typing, admissions, save/load, habitat placement, eating/drinking, viewing, keeper gates, Return to Menu | Uses the shared Freeform world, reloads its save once, then discards it |

The three optional diagnostics are `font-cache-revisit` (repeated allocation
measurements), `capture-readiness` (paired renders), and `frame-time-in-map`
(an isolated timing run). Repetition in these is the measurement itself.

Every capture retains the full fact set. Routine scripts assert player-visible
outcomes and ownership/settlement invariants, rather than repeating intermediate
mesh-loading and glyph-pipeline counters. Review screenshots for missing models,
clipped text and incorrect controls even when assertions pass.

This table describes coverage, not a passing release checklist. A failed gameplay
prerequisite blocks its dependent checks. Unsupported challenge UI, invalid gate
assets, placement failures and missing preview pixels remain failures; longer
waits or successful parser checks do not accept them.
