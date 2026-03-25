# Animation Reference

Three animation categories, verified from `render/animation/mod.rs` and submodules.

## Transitions

Play between slides when navigating. Blend old buffer (previous slide) and new buffer (next slide).

| Type | Directive | Duration | Effect |
|---|---|---|---|
| Fade | `<!-- transition: fade -->` | 400ms | Crossfade through background color |
| SlideLeft | `<!-- transition: slide -->` | 300ms | Old content slides left, new enters from right |
| Dissolve | `<!-- transition: dissolve -->` | 600ms | Per-character jumble into random symbols, then resolve |

Set globally in front matter with `transition: fade` or per-slide with the directive.

When both a transition and an entrance animation are set on a slide, the transition plays first with `exit_only=true` (only fades out old content), then the entrance reveals the new content.

## Entrance Animations

Play once when a slide first appears. Progressively reveal the new slide content.

| Type | Directive | Duration | Effect |
|---|---|---|---|
| Typewriter | `<!-- animation: typewriter -->` | 500ms | Characters appear left-to-right, one at a time |
| FadeIn | `<!-- animation: fade_in -->` | 500ms | All content fades from background to full brightness |
| SlideDown | `<!-- animation: slide_down -->` | 500ms | Lines revealed top-to-bottom, one row at a time |

## Loop Animations

Run continuously while a slide is displayed. Never complete -- replaced on slide change.

| Type | Directive | Effect |
|---|---|---|
| Matrix | `<!-- loop_animation: matrix -->` | Green cascading characters (Matrix rain) |
| Bounce | `<!-- loop_animation: bounce -->` | Bouncing ball in triangle-wave pattern |
| Pulse | `<!-- loop_animation: pulse -->` | Brightness oscillation via sine wave |
| Sparkle | `<!-- loop_animation: sparkle -->` | Random cells briefly become star characters |
| Spin | `<!-- loop_animation: spin -->` | ASCII brightness ramp cycling (shimmer wave) |

Multiple loop animations can be active on a single slide (use multiple directives).

## Animation Targeting

Loop animations accept an optional target to restrict the effect:

| Syntax | Target |
|---|---|
| `<!-- loop_animation: sparkle(figlet) -->` | Only animate FIGlet ASCII art title lines |
| `<!-- loop_animation: spin(image) -->` | Only animate ASCII art image lines |
| `<!-- loop_animation: pulse -->` | Animate all lines (no target) |

Targeting uses the `LineContentType` system: each `StyledLine` is tagged as `Normal`, `FigletTitle`, or `AsciiImage`. Animation dispatch checks the tag and skips non-matching lines.

## Animation Lifecycle

1. User navigates to a new slide
2. `on_slide_changed()` is called
3. `start_slide_animations()` creates the appropriate `AnimationState`
4. Event loop calls `tick()` and `render_frame()` on each iteration (~30fps)
5. Transition completes -> chains to entrance if present
6. Entrance completes -> discarded
7. Loop animations run indefinitely via frame counter (not progress-based)

## Timing

- Event loop uses 33ms poll timeout when animations are active (~30fps)
- 100ms poll timeout when idle (saves CPU, still updates timer)
- Transitions use `Instant::now().elapsed()` for smooth time-based progress
- Loop animations use a monotonically increasing frame counter
